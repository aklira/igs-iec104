// The TLS station of the second reference peer (IMPLEMENTATION.md §13; task X1). It is a small program of
// this project that drives the OpenMUC j60870 library through its public API, as a black box: it listens
// on a port over TLS (IEC 62351-3), confirms each interrogation, and answers it with one measured value.
// It keeps no state beyond that value, and it has no protocol logic of its own: the library does the framing,
// the link, and the transfer state.
//
// The station's identity is a PKCS12 keystore. The client certificate is not required by this program.
//
// Usage: java -cp classes:j60870.jar TlsStation --port 2404 --common-address 1 \
//        --keystore station.p12 --password PASSWORD

import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.KeyStore;
import javax.net.ssl.KeyManagerFactory;
import javax.net.ssl.SSLContext;
import org.openmuc.j60870.ASdu;
import org.openmuc.j60870.ASduType;
import org.openmuc.j60870.CauseOfTransmission;
import org.openmuc.j60870.Connection;
import org.openmuc.j60870.ConnectionEventListener;
import org.openmuc.j60870.Server;
import org.openmuc.j60870.ServerEventListener;
import org.openmuc.j60870.ie.IeQuality;
import org.openmuc.j60870.ie.InformationObject;
import org.openmuc.j60870.ie.IeShortFloat;
import org.openmuc.j60870.ie.InformationElement;

public final class TlsStation {

    /** The information object address of the point, and its value. */
    static final int POINT_ADDRESS = 100;
    static final float POINT_VALUE = 12.5f;

    private TlsStation() {}

    public static void main(String[] args) throws Exception {
        int port = 2404;
        int commonAddress = 1;
        String keystore = null;
        String password = "";
        for (int i = 0; i + 1 < args.length; i += 2) {
            switch (args[i]) {
                case "--port" -> port = Integer.parseInt(args[i + 1]);
                case "--common-address" -> commonAddress = Integer.parseInt(args[i + 1]);
                case "--keystore" -> keystore = args[i + 1];
                case "--password" -> password = args[i + 1];
                default -> throw new IllegalArgumentException("unknown option " + args[i]);
            }
        }
        if (keystore == null) {
            System.err.println("the station identity is required: --keystore");
            System.exit(2);
        }
        final int address = commonAddress;
        Server server = Server.builder()
                .setPort(port)
                .setSocketFactory(serverFactory(keystore, password.toCharArray()))
                .build();
        server.start(new ServerEventListener() {
            @Override
            public ConnectionEventListener connectionIndication(Connection connection) {
                return listener(address);
            }

            @Override
            public void serverStoppedListeningIndication(IOException e) {
                System.err.println("the station stopped listening: " + e.getMessage());
            }

            @Override
            public void connectionAttemptFailed(IOException e) {
                System.err.println("a connection attempt failed: " + e.getMessage());
            }
        });
        System.out.println("station listening on port " + port + " over TLS with common address " + address);
        Thread.currentThread().join();
    }

    /** The server socket factory of the station: TLS with the station's identity. */
    private static javax.net.ServerSocketFactory serverFactory(String keystore, char[] password) throws Exception {
        KeyStore store = KeyStore.getInstance("PKCS12");
        try (InputStream in = Files.newInputStream(Path.of(keystore))) {
            store.load(in, password);
        }
        KeyManagerFactory managers = KeyManagerFactory.getInstance(KeyManagerFactory.getDefaultAlgorithm());
        managers.init(store, password);
        SSLContext context = SSLContext.getInstance("TLS");
        context.init(managers.getKeyManagers(), null, null);
        return context.getServerSocketFactory();
    }

    private static ConnectionEventListener listener(int commonAddress) {
        return new ConnectionEventListener() {
            @Override
            public void newASdu(Connection connection, ASdu asdu) {
                if (asdu.getTypeIdentification() != ASduType.C_IC_NA_1) {
                    return;
                }
                try {
                    // The activation of the interrogation is confirmed, then the point is sent in
                    // answer to it, then the interrogation is terminated (IEC 60870-5-104 §7.5).
                    connection.sendConfirmation(asdu);
                    InformationElement[][] elements = {
                        {new IeShortFloat(POINT_VALUE), new IeQuality(false, false, false, false, false)}
                    };
                    connection.send(
                            new ASdu(
                                    ASduType.M_ME_NC_1,
                                    false,
                                    CauseOfTransmission.INTERROGATED_BY_STATION,
                                    false,
                                    false,
                                    0,
                                    commonAddress,
                                    new InformationObject(POINT_ADDRESS, elements)));
                    connection.sendActivationTermination(asdu);
                } catch (IOException e) {
                    System.err.println("the answer could not be sent: " + e.getMessage());
                }
            }

            @Override
            public void dataTransferStateChanged(Connection connection, boolean stopped) {
                System.out.println("data transfer " + (stopped ? "stopped" : "started"));
            }

            @Override
            public void connectionClosed(Connection connection, IOException cause) {
                System.out.println("connection closed: " + cause.getMessage());
            }
        };
    }
}
