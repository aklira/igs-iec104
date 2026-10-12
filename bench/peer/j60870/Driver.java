// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

// The independent peer of the S5 interop matrix (IMPLEMENTATION.md §13; task S5). It is a controlling
// station of this project that drives the OpenMUC j60870 library (GNU GPL v3,
// https://www.openmuc.org/j60870/) through its public API, as a black box. It runs one procedure
// against the station under test and prints what the station answers. It keeps no protocol logic of
// its own: the outcome is read from the answers the library hands to the listener.
//
// Usage: java -cp classes:j60870.jar Driver --host 172.31.104.40 --port 2404 --common-address 45 \
//        --procedure single
//
// Output: one line per answer ("asdu ..."), then one line per procedure:
//   OUTCOME <procedure> <passed|refused|timeout> <detail>
// Exit status 0 when the outcome line was printed, 2 for a usage error, 1 when the connection failed.

import java.io.IOException;
import java.util.List;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.function.Predicate;
import org.openmuc.j60870.ASdu;
import org.openmuc.j60870.ASduType;
import org.openmuc.j60870.CauseOfTransmission;
import org.openmuc.j60870.ClientConnectionBuilder;
import org.openmuc.j60870.Connection;
import org.openmuc.j60870.ConnectionEventListener;
import org.openmuc.j60870.ie.IeDoubleCommand;
import org.openmuc.j60870.ie.IeNormalizedValue;
import org.openmuc.j60870.ie.IeQualifierOfCounterInterrogation;
import org.openmuc.j60870.ie.IeQualifierOfInterrogation;
import org.openmuc.j60870.ie.IeQualifierOfResetProcessCommand;
import org.openmuc.j60870.ie.IeQualifierOfSetPointCommand;
import org.openmuc.j60870.ie.IeScaledValue;
import org.openmuc.j60870.ie.IeShortFloat;
import org.openmuc.j60870.ie.IeTestSequenceCounter;
import org.openmuc.j60870.ie.IeSingleCommand;
import org.openmuc.j60870.ie.IeTime56;
import org.openmuc.j60870.ie.InformationObject;

public final class Driver {

    /** The station interrogation, qualifier 20 (IEC 60870-5-104 §7.5, the value used as data). */
    static final int STATION_INTERROGATION = 20;
    /** The general counter request, and no freeze (the counter interrogation qualifier). */
    static final int COUNTER_GENERAL_REQUEST = 5;
    /** The test pattern of the test command (the value the station answers with). */
    static final int TEST_PATTERN = 0x55AA;
    static final long PATIENCE_SECONDS = 10;

    /** Points of the S5 station (bench/peer/s5/points.toml): a plain point, then the command points. */
    static final int POINT_PLAIN = 672;
    static final int COMMAND_SINGLE = 10005;
    static final int COMMAND_DOUBLE = 10006;
    static final int COMMAND_NORMALIZED = 10008;
    static final int COMMAND_SCALED = 10009;
    static final int COMMAND_FLOAT = 10010;

    private final List<ASdu> answers = new CopyOnWriteArrayList<>();
    private final CountDownLatch transferStarted = new CountDownLatch(1);
    private Connection connection;

    private Driver() {}

    public static void main(String[] args) throws Exception {
        String host = null;
        String procedure = null;
        int port = 2404;
        int commonAddress = 1;
        for (int i = 0; i + 1 < args.length; i += 2) {
            switch (args[i]) {
                case "--host" -> host = args[i + 1];
                case "--port" -> port = Integer.parseInt(args[i + 1]);
                case "--common-address" -> commonAddress = Integer.parseInt(args[i + 1]);
                case "--procedure" -> procedure = args[i + 1];
                default -> throw new IllegalArgumentException("unknown option " + args[i]);
            }
        }
        if (host == null || procedure == null) {
            System.err.println("usage: Driver --host ADDRESS --procedure NAME [--port N] [--common-address N]");
            System.exit(2);
        }
        Driver driver = new Driver();
        System.exit(driver.run(host, port, commonAddress, procedure));
    }

    private int run(String host, int port, int commonAddress, String procedure) throws Exception {
        ConnectionEventListener listener = new ConnectionEventListener() {
            @Override
            public void newASdu(Connection connection, ASdu asdu) {
                answers.add(asdu);
                System.out.println("asdu " + describe(asdu));
            }

            @Override
            public void dataTransferStateChanged(Connection connection, boolean stopped) {
                if (!stopped) {
                    transferStarted.countDown();
                }
            }

            @Override
            public void connectionClosed(Connection connection, IOException cause) {
                System.out.println("closed " + cause.getMessage());
            }
        };
        connection = new ClientConnectionBuilder(host)
                .setPort(port)
                .setConnectionEventListener(listener)
                .build();
        try {
            connection.startDataTransfer();
            if (!transferStarted.await(PATIENCE_SECONDS, TimeUnit.SECONDS)) {
                System.err.println("the data transfer did not start");
                return 1;
            }
            Outcome outcome = perform(procedure, commonAddress);
            System.out.println("OUTCOME " + procedure + " " + outcome.kind + " " + outcome.detail);
            return 0;
        } finally {
            connection.close();
        }
    }

    /** What a procedure came to: the answers that decided it. */
    private record Outcome(String kind, String detail) {}

    private Outcome perform(String procedure, int ca) throws Exception {
        int mark = answers.size();
        switch (procedure) {
            case "interrogation" -> {
                connection.interrogation(ca, CauseOfTransmission.ACTIVATION,
                        new IeQualifierOfInterrogation(STATION_INTERROGATION));
                return until(mark, ASduType.C_IC_NA_1, CauseOfTransmission.ACTIVATION_TERMINATION);
            }
            case "counter" -> {
                connection.counterInterrogation(ca, CauseOfTransmission.ACTIVATION,
                        new IeQualifierOfCounterInterrogation(COUNTER_GENERAL_REQUEST, 0));
                return until(mark, ASduType.C_CI_NA_1, CauseOfTransmission.ACTIVATION_TERMINATION);
            }
            case "read" -> {
                connection.readCommand(ca, POINT_PLAIN);
                ASdu answer = await(mark, a -> a.getCauseOfTransmission() == CauseOfTransmission.REQUEST
                        || a.isNegativeConfirm());
                return answer == null ? timeout() : outcomeOf(answer);
            }
            case "clock" -> {
                connection.synchronizeClocks(ca, new IeTime56(System.currentTimeMillis()));
                return confirmation(mark, ASduType.C_CS_NA_1);
            }
            case "test" -> {
                connection.testCommandWithTimeTag(ca, new IeTestSequenceCounter(TEST_PATTERN),
                        new IeTime56(System.currentTimeMillis()));
                return confirmation(mark, ASduType.C_TS_TA_1);
            }
            case "test-without-time-tag" -> {
                connection.testCommand(ca);
                return confirmation(mark, ASduType.C_TS_NA_1);
            }
            case "reset" -> {
                connection.resetProcessCommand(ca, new IeQualifierOfResetProcessCommand(1));
                return confirmation(mark, ASduType.C_RP_NA_1);
            }
            case "single" -> {
                return command(ca, ASduType.C_SC_NA_1, COMMAND_SINGLE);
            }
            case "double" -> {
                return command(ca, ASduType.C_DC_NA_1, COMMAND_DOUBLE);
            }
            case "normalized" -> {
                return command(ca, ASduType.C_SE_NA_1, COMMAND_NORMALIZED);
            }
            case "scaled" -> {
                return command(ca, ASduType.C_SE_NB_1, COMMAND_SCALED);
            }
            case "float" -> {
                return command(ca, ASduType.C_SE_NC_1, COMMAND_FLOAT);
            }
            case "single-without-command" -> {
                return command(ca, ASduType.C_SC_NA_1, POINT_PLAIN);
            }
            default -> throw new IllegalArgumentException("unknown procedure " + procedure);
        }
    }

    /** A command: select, then execute once the selection is confirmed (IEC 60870-5-104 §7.2.6). */
    private Outcome command(int ca, ASduType type, int ioa) throws Exception {
        int mark = answers.size();
        send(ca, type, ioa, true);
        ASdu selected = await(mark, answerTo(type));
        if (selected == null) {
            return timeout();
        }
        if (selected.isNegativeConfirm()) {
            return refused(selected);
        }
        int executing = answers.size();
        send(ca, type, ioa, false);
        ASdu executed = await(executing, answerTo(type));
        if (executed == null) {
            return timeout();
        }
        if (executed.isNegativeConfirm()) {
            return refused(executed);
        }
        ASdu terminated = await(executing, a -> a.getTypeIdentification() == type
                && a.getCauseOfTransmission() == CauseOfTransmission.ACTIVATION_TERMINATION);
        return terminated == null ? timeout() : new Outcome("passed", "selected, executed and terminated");
    }

    private void send(int ca, ASduType type, int ioa, boolean select) throws IOException {
        CauseOfTransmission activation = CauseOfTransmission.ACTIVATION;
        switch (type) {
            case C_SC_NA_1 -> connection.singleCommand(ca, activation, ioa,
                    new IeSingleCommand(true, 0, select));
            case C_DC_NA_1 -> connection.doubleCommand(ca, activation, ioa,
                    new IeDoubleCommand(IeDoubleCommand.DoubleCommandState.ON, 0, select));
            case C_SE_NA_1 -> connection.setNormalizedValueCommand(ca, activation, ioa,
                    new IeNormalizedValue(0.5), new IeQualifierOfSetPointCommand(0, select));
            case C_SE_NB_1 -> connection.setScaledValueCommand(ca, activation, ioa,
                    new IeScaledValue(1234), new IeQualifierOfSetPointCommand(0, select));
            case C_SE_NC_1 -> connection.setShortFloatCommand(ca, activation, ioa,
                    new IeShortFloat(12.5f), new IeQualifierOfSetPointCommand(0, select));
            default -> throw new IllegalArgumentException("not a command in this matrix: " + type);
        }
    }

    /** A procedure answered by one confirmation: positive, or refused by a negative one. */
    private Outcome confirmation(int mark, ASduType type) throws InterruptedException {
        ASdu answer = await(mark, answerTo(type));
        if (answer == null) {
            return timeout();
        }
        return answer.isNegativeConfirm() ? refused(answer) : new Outcome("passed", "confirmed");
    }

    /** A procedure that ends with a termination, after its confirmation. */
    private Outcome until(int mark, ASduType type, CauseOfTransmission end) throws InterruptedException {
        ASdu confirmed = await(mark, answerTo(type));
        if (confirmed == null) {
            return timeout();
        }
        if (confirmed.isNegativeConfirm()) {
            return refused(confirmed);
        }
        ASdu terminated = await(mark, a -> a.getTypeIdentification() == type
                && a.getCauseOfTransmission() == end);
        return terminated == null ? timeout() : new Outcome("passed", "confirmed and terminated");
    }

    /** The confirmation of a request of this type: positive (activation confirmation), or negative with any cause. */
    private static Predicate<ASdu> answerTo(ASduType type) {
        return a -> a.getTypeIdentification() == type
                && (a.getCauseOfTransmission() == CauseOfTransmission.ACTIVATION_CON || a.isNegativeConfirm());
    }

    private Outcome outcomeOf(ASdu answer) {
        return answer.isNegativeConfirm() ? refused(answer) : new Outcome("passed", describe(answer));
    }

    private static Outcome refused(ASdu answer) {
        return new Outcome("refused", "cause " + answer.getCauseOfTransmission().getId()
                + " type " + answer.getTypeIdentification());
    }

    private static Outcome timeout() {
        return new Outcome("timeout", "no answer within " + PATIENCE_SECONDS + " s");
    }

    /** Waits for an answer after the mark, the first one the predicate accepts. */
    private ASdu await(int mark, Predicate<ASdu> wanted) throws InterruptedException {
        long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(PATIENCE_SECONDS);
        while (System.nanoTime() < deadline) {
            for (int i = mark; i < answers.size(); i++) {
                if (wanted.test(answers.get(i))) {
                    return answers.get(i);
                }
            }
            Thread.sleep(20);
        }
        return null;
    }

    private static String describe(ASdu asdu) {
        StringBuilder points = new StringBuilder();
        for (InformationObject object : asdu.getInformationObjects()) {
            if (points.length() > 0) {
                points.append(',');
            }
            points.append(object.getInformationObjectAddress());
        }
        return "type=" + asdu.getTypeIdentification()
                + " cause=" + asdu.getCauseOfTransmission().getId()
                + " negative=" + asdu.isNegativeConfirm()
                + " ca=" + asdu.getCommonAddress()
                + " ioa=" + points;
    }
}
