package com.dubborst.fixtures;

import io.grpc.Server;
import io.grpc.ServerBuilder;
import io.grpc.stub.StreamObserver;
import java.io.IOException;
import triple.test.TripleServiceGrpc;
import triple.test.TripleTest.TripleRequestWrapper;
import triple.test.TripleTest.TripleResponseWrapper;

public class TripleTestServer {

    private final int port;
    private Server server;

    public TripleTestServer(int port) {
        this.port = port;
    }

    public TripleTestServer() {
        this(50051);
    }

    public TripleTestServer start() throws IOException {
        server = ServerBuilder.forPort(port)
                .addService(new TripleServiceImpl())
                .build()
                .start();
        System.out.println("[TripleTestServer] Started on port " + port);
        return this;
    }

    public void shutdown() {
        if (server != null) {
            server.shutdown();
            System.out.println("[TripleTestServer] Shutdown complete");
        }
    }

    public void blockUntilShutdown() throws InterruptedException {
        if (server != null) {
            server.awaitTermination();
        }
    }

    public int getPort() {
        return port;
    }

    private static class TripleServiceImpl extends TripleServiceGrpc.TripleServiceImplBase {

        @Override
        public void echo(TripleRequestWrapper request, StreamObserver<TripleResponseWrapper> responseObserver) {
            String reply = "pong";
            TripleResponseWrapper response = TripleResponseWrapper.newBuilder()
                    .setSerializeType(request.getSerializeType())
                    .setData(com.google.protobuf.ByteString.copyFromUtf8(reply))
                    .setType("")
                    .build();
            responseObserver.onNext(response);
            responseObserver.onCompleted();
        }

        @Override
        public void sayHello(TripleRequestWrapper request, StreamObserver<TripleResponseWrapper> responseObserver) {
            String greeting = "Hello from Java Triple! args=" + request.getArgsCount();
            TripleResponseWrapper response = TripleResponseWrapper.newBuilder()
                    .setSerializeType(request.getSerializeType())
                    .setData(com.google.protobuf.ByteString.copyFromUtf8(greeting))
                    .setType("")
                    .build();
            responseObserver.onNext(response);
            responseObserver.onCompleted();
        }

        @Override
        public void serverStreamEcho(TripleRequestWrapper request, StreamObserver<TripleResponseWrapper> responseObserver) {
            for (int i = 1; i <= 3; i++) {
                TripleResponseWrapper response = TripleResponseWrapper.newBuilder()
                        .setSerializeType(request.getSerializeType())
                        .setData(com.google.protobuf.ByteString.copyFromUtf8("chunk" + i))
                        .setType("")
                        .build();
                responseObserver.onNext(response);
            }
            responseObserver.onCompleted();
        }
    }

    public static void main(String[] args) throws Exception {
        int port = args.length > 0 ? Integer.parseInt(args[0]) : 50051;
        TripleTestServer server = new TripleTestServer(port);
        Runtime.getRuntime().addShutdownHook(new Thread(server::shutdown));
        server.start();
        System.out.println("[TripleTestServer] Press Ctrl+C to stop");
        server.blockUntilShutdown();
    }
}
