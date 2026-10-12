// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

// The load client of the S5 load test (IMPLEMENTATION.md §13; task S5). It drives the OpenMUC j60870
// library (GNU GPL v3) through its public API, as a black box, against the station under test. It prints
// the measurements and keeps no protocol logic of its own: Python computes the percentiles and checks them.
//
// Modes:
//   gi      --sessions N: N sessions, one after the other, each with a station interrogation. A station
//           starts one connection at a time (docs/igs104-server.md), and the changes that wait for a
//           connection are sent first on the next one, so these figures include that backlog.
//   failover --seconds S: one connection for S seconds, then a second connection for S seconds that starts
//           the data transfer and so supersedes the first (the station's redundancy group, §10.7). Printed:
//           "value ioa=<address> v=<value> conn=<A|B>" per time-tagged change, after the session.
//   session --seconds S --every G --commands K: one connection of S seconds. A station interrogation is
//           sent every G seconds, and between two of them a single command is sent every 500 ms on one of K
//           command points (select, then execute). Printed: "gi ms=<to the termination> points=<n>" per
//           interrogation, "spont ms=<now - time tag>" per time-tagged change, "cmd ms=<execution to its
//           confirmation>" per command, and "cmd refused" or "cmd failed" for a command that did not succeed.
//
// Usage: java -cp classes:j60870.jar Load --host 172.31.104.50 --port 2404 --common-address 45 --mode session

import java.io.IOException;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import org.openmuc.j60870.ASdu;
import org.openmuc.j60870.ASduType;
import org.openmuc.j60870.CauseOfTransmission;
import org.openmuc.j60870.ClientConnectionBuilder;
import org.openmuc.j60870.Connection;
import org.openmuc.j60870.ConnectionEventListener;
import org.openmuc.j60870.ie.IeQualifierOfInterrogation;
import org.openmuc.j60870.ie.IeShortFloat;
import org.openmuc.j60870.ie.IeSingleCommand;
import org.openmuc.j60870.ie.IeTime56;
import org.openmuc.j60870.ie.InformationElement;
import org.openmuc.j60870.ie.InformationObject;

public final class Load {

    /** The station interrogation, qualifier 20 (the value used as data). */
    static final int STATION_INTERROGATION = 20;
    static final long PATIENCE_SECONDS = 120;
    static final long COMMAND_PATIENCE_SECONDS = 10;
    static final long COMMAND_EVERY_MS = 500;
    /** First address of the command points of the load station (bench/load, written by the test). */
    static final int FIRST_COMMAND_POINT = 20001;
    static final int TOTAL_POINTS = 10010;

    private final Set<Integer> answered = ConcurrentHashMap.newKeySet();
    private final AtomicInteger received = new AtomicInteger();
    private volatile CountDownLatch started = new CountDownLatch(1);
    private volatile CountDownLatch terminated = new CountDownLatch(1);
    private volatile CountDownLatch pending;
    private volatile boolean pendingRefused;
    private volatile boolean collecting;
    private volatile boolean spontaneousOn;
    /** Values of the time-tagged changes, as "ioa value connection", printed at the end of a failover. */
    private final java.util.Queue<String> values = new java.util.concurrent.ConcurrentLinkedQueue<>();
    private volatile String connectionLabel = "A";
    /** Changes as {latency, tag, received, address}, printed at the end: no print in the reading thread. */
    private final java.util.Queue<long[]> changes = new java.util.concurrent.ConcurrentLinkedQueue<>();

    private Load() {}

    public static void main(String[] args) throws Exception {
        String host = null;
        String mode = null;
        int port = 2404;
        int commonAddress = 1;
        int sessions = 1;
        int seconds = 10;
        int every = 10;
        int commands = 10;
        for (int i = 0; i + 1 < args.length; i += 2) {
            switch (args[i]) {
                case "--host" -> host = args[i + 1];
                case "--port" -> port = Integer.parseInt(args[i + 1]);
                case "--common-address" -> commonAddress = Integer.parseInt(args[i + 1]);
                case "--mode" -> mode = args[i + 1];
                case "--sessions" -> sessions = Integer.parseInt(args[i + 1]);
                case "--seconds" -> seconds = Integer.parseInt(args[i + 1]);
                case "--every" -> every = Integer.parseInt(args[i + 1]);
                case "--commands" -> commands = Integer.parseInt(args[i + 1]);
                default -> throw new IllegalArgumentException("unknown option " + args[i]);
            }
        }
        if (host == null || mode == null) {
            System.err.println("usage: Load --host ADDRESS --mode gi|session [--sessions N] [--seconds S] [--every G] [--commands K]");
            System.exit(2);
        }
        Load load = new Load();
        int status = switch (mode) {
            case "gi" -> load.generalInterrogations(host, port, commonAddress, sessions);
            case "session" -> load.session(host, port, commonAddress, seconds, every, commands);
            case "failover" -> load.failover(host, port, commonAddress, seconds);
            default -> throw new IllegalArgumentException("unknown mode " + mode);
        };
        System.exit(status);
    }

    private int generalInterrogations(String host, int port, int ca, int sessions) throws Exception {
        for (int session = 1; session <= sessions; session++) {
            started = new CountDownLatch(1);
            Connection link = open(host, port);
            try {
                link.startDataTransfer();
                if (!started.await(PATIENCE_SECONDS, TimeUnit.SECONDS)) {
                    System.out.println("gi session=" + session + " failed the data transfer did not start");
                    return 1;
                }
                if (!interrogate(link, ca, "session=" + session)) {
                    return 1;
                }
            } finally {
                collecting = false;
                link.stopDataTransfer();
                link.close();
            }
        }
        return 0;
    }

    private int session(String host, int port, int ca, int seconds, int every, int commands) throws Exception {
        spontaneousOn = true;
        started = new CountDownLatch(1);
        Connection link = open(host, port);
        try {
            link.startDataTransfer();
            if (!started.await(PATIENCE_SECONDS, TimeUnit.SECONDS)) {
                System.out.println("session failed the data transfer did not start");
                return 1;
            }
            long end = System.nanoTime() + TimeUnit.SECONDS.toNanos(seconds);
            long nextInterrogation = System.nanoTime();
            int next = 0;
            while (System.nanoTime() < end) {
                if (System.nanoTime() >= nextInterrogation) {
                    if (!interrogate(link, ca, "load")) {
                        return 1;
                    }
                    nextInterrogation += TimeUnit.SECONDS.toNanos(every);
                } else {
                    command(link, ca, FIRST_COMMAND_POINT + (next++ % commands));
                    Thread.sleep(COMMAND_EVERY_MS);
                }
            }
            for (long[] change : changes) {
                System.out.println("spont ms=" + change[0] + " tag=" + change[1] + " now=" + change[2]
                        + " ioa=" + change[3]);
            }
            System.out.println("session done seconds=" + seconds);
            return 0;
        } finally {
            spontaneousOn = false;
            link.stopDataTransfer();
            link.close();
        }
    }

    private int failover(String host, int port, int ca, int seconds) throws Exception {
        spontaneousOn = true;
        started = new CountDownLatch(1);
        Connection first = open(host, port);
        first.startDataTransfer();
        if (!started.await(PATIENCE_SECONDS, TimeUnit.SECONDS)) {
            System.out.println("failover failed the first data transfer did not start");
            return 1;
        }
        Thread.sleep(TimeUnit.SECONDS.toMillis(seconds));
        started = new CountDownLatch(1);
        connectionLabel = "B";
        Connection second = open(host, port);
        second.startDataTransfer();
        if (!started.await(PATIENCE_SECONDS, TimeUnit.SECONDS)) {
            System.out.println("failover failed the second data transfer did not start");
            return 1;
        }
        Thread.sleep(TimeUnit.SECONDS.toMillis(seconds));
        spontaneousOn = false;
        for (String value : values) {
            System.out.println("value " + value);
        }
        System.out.println("failover done seconds=" + seconds);
        first.close();
        second.stopDataTransfer();
        second.close();
        return 0;
    }

    /** One station interrogation: printed with its time to the termination and the points it answered. */
    private boolean interrogate(Connection link, int ca, String label) throws Exception {
        answered.clear();
        terminated = new CountDownLatch(1);
        collecting = true;
        long began = System.nanoTime();
        link.interrogation(ca, CauseOfTransmission.ACTIVATION,
                new IeQualifierOfInterrogation(STATION_INTERROGATION));
        boolean ended = terminated.await(PATIENCE_SECONDS, TimeUnit.SECONDS);
        collecting = false;
        if (!ended) {
            System.out.println("gi " + label + " failed no termination");
            return false;
        }
        long ms = TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - began);
        System.out.println("gi " + label + " ms=" + ms + " points=" + answered.size());
        return true;
    }

    /** Selects, then executes, a single command, and prints the time from the execution to its confirmation. */
    private void command(Connection link, int ca, int ioa) throws Exception {
        int mark = received.get();
        CountDownLatch selected = new CountDownLatch(1);
        pendingRefused = false;
        pending = selected;
        link.singleCommand(ca, CauseOfTransmission.ACTIVATION, ioa, new IeSingleCommand(true, 0, true));
        if (!selected.await(COMMAND_PATIENCE_SECONDS, TimeUnit.SECONDS)) {
            System.out.println("cmd failed no selection for " + ioa + " asdus since="
                    + (received.get() - mark));
            pending = null;
            return;
        }
        if (pendingRefused) {
            System.out.println("cmd refused selection " + ioa);
            pending = null;
            return;
        }
        CountDownLatch executed = new CountDownLatch(1);
        pendingRefused = false;
        pending = executed;
        long sent = System.nanoTime();
        link.singleCommand(ca, CauseOfTransmission.ACTIVATION, ioa, new IeSingleCommand(true, 0, false));
        if (!executed.await(COMMAND_PATIENCE_SECONDS, TimeUnit.SECONDS)) {
            System.out.println("cmd failed no execution for " + ioa + " asdus since="
                    + (received.get() - mark));
        } else if (pendingRefused) {
            System.out.println("cmd refused execution " + ioa);
        } else {
            System.out.println("cmd ms=" + TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - sent)
                    + " ioa=" + ioa);
        }
        pending = null;
    }

    private Connection open(String host, int port) throws IOException {
        ConnectionEventListener listener = new ConnectionEventListener() {
            @Override
            public void newASdu(Connection connection, ASdu asdu) {
                received.incrementAndGet();
                onAsdu(asdu);
            }

            @Override
            public void dataTransferStateChanged(Connection connection, boolean stopped) {
                if (!stopped) {
                    started.countDown();
                }
            }

            @Override
            public void connectionClosed(Connection connection, IOException cause) {
                // The session ends with its own close; a station-side close shows as a failed session.
            }
        };
        return new ClientConnectionBuilder(host)
                .setPort(port)
                .setConnectionEventListener(listener)
                .build();
    }

    private void onAsdu(ASdu asdu) {
        ASduType type = asdu.getTypeIdentification();
        CauseOfTransmission cause = asdu.getCauseOfTransmission();
        if (type == ASduType.C_IC_NA_1 && cause == CauseOfTransmission.ACTIVATION_TERMINATION) {
            terminated.countDown();
            return;
        }
        if (type == ASduType.C_SC_NA_1) {
            CountDownLatch latch = pending;
            if (latch != null && (cause == CauseOfTransmission.ACTIVATION_CON || asdu.isNegativeConfirm())) {
                pendingRefused = asdu.isNegativeConfirm();
                latch.countDown();
            }
            return;
        }
        if (collecting && cause == CauseOfTransmission.INTERROGATED_BY_STATION) {
            for (InformationObject object : asdu.getInformationObjects()) {
                if (object.getInformationObjectAddress() > 0) {
                    answered.add(object.getInformationObjectAddress());
                }
            }
        }
        if (spontaneousOn && type == ASduType.M_ME_TF_1) {
            long now = System.currentTimeMillis();
            for (InformationObject object : asdu.getInformationObjects()) {
                for (InformationElement element : object.getInformationElements()[0]) {
                    if (element instanceof IeShortFloat value) {
                        values.add("ioa=" + object.getInformationObjectAddress() + " v=" + value.getValue()
                                + " conn=" + connectionLabel);
                    }
                    if (element instanceof IeTime56 stamp) {
                        changes.add(new long[] {now - stamp.getTimestamp(), stamp.getTimestamp(), now,
                                object.getInformationObjectAddress()});
                    }
                }
            }
        }
    }
}
