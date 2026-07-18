/*
 * Licensed to the Apache Software Foundation (ASF) under one or more
 * contributor license agreements. See the NOTICE file distributed with
 * this work for additional information regarding copyright ownership.
 * The ASF licenses this file to You under the Apache License, Version 2.0
 * (the "License"); you may not use this file except in compliance with
 * the License. You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 */
package com.dubborst.fixtures;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.net.InetSocketAddress;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import com.alibaba.com.caucho.hessian.io.Hessian2Input;
import com.alibaba.com.caucho.hessian.io.Hessian2Output;

import io.netty.bootstrap.ServerBootstrap;
import io.netty.buffer.ByteBuf;
import io.netty.channel.ChannelFuture;
import io.netty.channel.ChannelHandlerContext;
import io.netty.channel.ChannelInboundHandlerAdapter;
import io.netty.channel.ChannelInitializer;
import io.netty.channel.ChannelOption;
import io.netty.channel.EventLoopGroup;
import io.netty.channel.nio.NioEventLoopGroup;
import io.netty.channel.socket.SocketChannel;
import io.netty.channel.socket.nio.NioServerSocketChannel;
import io.netty.handler.codec.ByteToMessageDecoder;
import io.netty.handler.codec.MessageToByteEncoder;

/**
 * Simple Netty-based Dubbo TCP test server skeleton.
 *
 * <p>Handles the Dubbo2 binary protocol (16-byte header + Hessian2 body).
 * Used for cross-language verification: dubbo-rs consumer → this Java provider.</p>
 *
 * <h3>Protocol Header (16 bytes)</h3>
 * <pre>
 *   Offset  Size  Field
 *   0       2     Magic      (0xdabb)
 *   2       1     Flags      (request/response/twoway/event/serialId)
 *   3       1     Status     (response status, 0 for requests)
 *   4       8     RequestId  (unique id)
 *   12      4     BodyLength (body size in bytes)
 * </pre>
 *
 * <p>Usage:</p>
 * <pre>
 *   DubboTcpTestServer server = new DubboTcpTestServer(20880);
 *   server.start();   // non-blocking
 *   // ... run tests ...
 *   server.shutdown();
 * </pre>
 */
public class DubboTcpTestServer {

    private static final short DUBBO_MAGIC = (short) 0xDABB;
    private static final int HEADER_LENGTH = 16;

    private final int port;
    private EventLoopGroup bossGroup;
    private EventLoopGroup workerGroup;
    private ChannelFuture channelFuture;

    public DubboTcpTestServer(int port) {
        this.port = port;
    }

    public DubboTcpTestServer() {
        this(20880);
    }

    /**
     * Start the server (non-blocking).
     *
     * @return this server instance
     */
    public DubboTcpTestServer start() {
        bossGroup = new NioEventLoopGroup(1);
        workerGroup = new NioEventLoopGroup();

        ServerBootstrap bootstrap = new ServerBootstrap();
        bootstrap.group(bossGroup, workerGroup)
                .channel(NioServerSocketChannel.class)
                .childHandler(new DubboChannelInitializer())
                .option(ChannelOption.SO_BACKLOG, 128)
                .childOption(ChannelOption.SO_KEEPALIVE, true);

        channelFuture = bootstrap.bind(new InetSocketAddress(port)).syncUninterruptibly();
        System.out.println("[DubboTcpTestServer] Listening on port " + port);
        return this;
    }

    /**
     * Shutdown the server and release resources.
     */
    public void shutdown() {
        if (channelFuture != null) {
            channelFuture.channel().close().syncUninterruptibly();
        }
        if (workerGroup != null) {
            workerGroup.shutdownGracefully();
        }
        if (bossGroup != null) {
            bossGroup.shutdownGracefully();
        }
        System.out.println("[DubboTcpTestServer] Shutdown complete");
    }

    /**
     * Get the port the server is listening on.
     */
    public int getPort() {
        return port;
    }

    // ── Netty Channel Pipeline ───────────────────────────────────────────

    private static class DubboChannelInitializer extends ChannelInitializer<SocketChannel> {
        @Override
        protected void initChannel(SocketChannel ch) {
            ch.pipeline()
                    .addLast("decoder", new DubboProtocolDecoder())
                    .addLast("encoder", new DubboProtocolEncoder())
                    .addLast("handler", new DubboRequestHandler());
        }
    }

    // ── Decoder: reads 16-byte header + body ─────────────────────────────

    private static class DubboProtocolDecoder extends ByteToMessageDecoder {
        @Override
        protected void decode(ChannelHandlerContext ctx, ByteBuf in, java.util.List<Object> out) {
            // Need at least 16 bytes for the header
            if (in.readableBytes() < HEADER_LENGTH) {
                return;
            }

            // Mark reader index so we can reset if body is incomplete
            in.markReaderIndex();

            // Read magic
            short magic = in.readShort();
            if (magic != DUBBO_MAGIC) {
                ctx.close();
                return;
            }

            // Read flags, status, requestId
            byte flags = in.readByte();
            byte status = in.readByte();
            long requestId = in.readLong();

            // Read body length
            int bodyLength = in.readInt();
            if (bodyLength < 0) {
                ctx.close();
                return;
            }

            // Wait until full body is available
            if (in.readableBytes() < bodyLength) {
                in.resetReaderIndex();
                return;
            }

            // Extract body bytes
            byte[] body = new byte[bodyLength];
            in.readBytes(body);

            DubboMessage message = new DubboMessage();
            message.flags = flags;
            message.status = status;
            message.requestId = requestId;
            message.isRequest = (flags & 0x80) != 0;
            message.isTwoWay = (flags & 0x40) != 0;
            message.isEvent = (flags & 0x20) != 0;
            message.serialId = (byte) (flags & 0x1f);
            message.body = body;

            out.add(message);
        }
    }

    // ── Encoder: writes 16-byte header + body ────────────────────────────

    private static class DubboProtocolEncoder extends MessageToByteEncoder<DubboMessage> {
        @Override
        protected void encode(ChannelHandlerContext ctx, DubboMessage msg, ByteBuf out) {
            out.writeShort(DUBBO_MAGIC);
            out.writeByte(msg.flags);
            out.writeByte(msg.status);
            out.writeLong(msg.requestId);
            out.writeInt(msg.body.length);
            out.writeBytes(msg.body);
        }
    }

    // ── Request Handler ──────────────────────────────────────────────────

    private static class DubboRequestHandler extends ChannelInboundHandlerAdapter {
        @Override
        public void channelRead(ChannelHandlerContext ctx, Object msg) {
            if (msg instanceof DubboMessage dubboMsg) {
                System.out.printf("[DubboTcpTestServer] Received request: id=%d, flags=0x%02x, bodyLen=%d%n",
                        dubboMsg.requestId, dubboMsg.flags, dubboMsg.body.length);

                if (dubboMsg.isEvent) {
                    // Heartbeat or event — respond with empty event
                    DubboMessage eventAck = new DubboMessage();
                    eventAck.flags = 0x20; // event flag only
                    eventAck.status = 20;  // OK status
                    eventAck.requestId = dubboMsg.requestId;
                    eventAck.body = new byte[0];
                    ctx.writeAndFlush(eventAck);
                    return;
                }

                try {
                    byte[] responseBody = handleDubboRequest(dubboMsg);
                    DubboMessage response = new DubboMessage();
                    response.flags = (byte) (dubboMsg.serialId & 0x1f);
                    response.status = 20;
                    response.requestId = dubboMsg.requestId;
                    response.body = responseBody;
                    ctx.writeAndFlush(response);
                } catch (Exception e) {
                    System.err.println("[DubboTcpTestServer] Error handling request: " + e.getMessage());
                    e.printStackTrace();
                    // Return error response
                    DubboMessage errorResp = new DubboMessage();
                    errorResp.flags = (byte) (dubboMsg.serialId & 0x1f);
                    errorResp.status = 30; // SERVER_ERROR
                    errorResp.requestId = dubboMsg.requestId;
                    errorResp.body = encodeErrorResponse(e.getMessage());
                    ctx.writeAndFlush(errorResp);
                }
            }
        }

        /**
         * Parse the Dubbo TCP request body (Hessian2 format) and return a response.
         *
         * Request body format (7 Hessian2 fields):
         *   1. dubbo_version (string)
         *   2. service_path (string)
         *   3. service_version (string)
         *   4. method_name (string)
         *   5. param_types_desc (string)
         *   6. arguments (list of binary)
         *   7. attachments (map)
         *
         * Response body format:
         *   flag (int): 2 = value present, 1 = null, 0 = exception
         *   data (binary/null/string)
         */
        private byte[] handleDubboRequest(DubboMessage msg) throws IOException {
            Hessian2Input h2i = new Hessian2Input(new ByteArrayInputStream(msg.body));

            String dubboVersion = h2i.readString();
            String servicePath = h2i.readString();
            String serviceVersion = h2i.readString();
            String methodName = h2i.readString();
            @SuppressWarnings("unused")
            String paramTypes = h2i.readString();

            // Read arguments list using readObject (handles all list tag variants)
            @SuppressWarnings("unchecked")
            List<byte[]> args;
            try {
                args = (List<byte[]>) h2i.readObject();
            } catch (Exception e) {
                args = new ArrayList<>();
            }
            if (args == null) {
                args = new ArrayList<>();
            }

            // Read attachments map
            HashMap<String, String> attachments = new HashMap<>();
            try {
                @SuppressWarnings("unchecked")
                HashMap<String, String> map = (HashMap<String, String>) h2i.readObject();
                if (map != null) {
                    attachments = map;
                }
            } catch (Exception ignored) {
            }

            System.out.printf("[DubboTcpTestServer]  %s.%s(%d args, svc=%s ver=%s)%n",
                    servicePath, methodName, args.size(), dubboVersion, serviceVersion);

            // Dispatch to mock service
            String result = dispatch(servicePath, methodName, args, attachments);

            // Encode response: flag(2=value) + result as binary
            ByteArrayOutputStream baos = new ByteArrayOutputStream();
            Hessian2Output h2o = new Hessian2Output(baos);
            h2o.writeInt(2); // RESPONSE_VALUE
            h2o.writeBytes(result.getBytes("UTF-8"));
            h2o.flush();
            return baos.toByteArray();
        }

        private String dispatch(String servicePath, String methodName,
                                 List<byte[]> args, Map<String, String> attachments) {
            if ("sayHello".equals(methodName) && args.size() >= 1) {
                String name = new String(args.get(0));
                return "Hello, " + name + "! (from Java)";
            }
            if ("echo".equals(methodName) && args.size() >= 1) {
                return new String(args.get(0));
            }
            if ("ping".equals(methodName)) {
                return "pong";
            }

            // B-R-006: Multi-parameter method
            if ("add".equals(methodName) && args.size() >= 2) {
                try {
                    int a = Integer.parseInt(new String(args.get(0)));
                    int b = Integer.parseInt(new String(args.get(1)));
                    return String.valueOf(a + b);
                } catch (NumberFormatException e) {
                    return "error:invalid_number";
                }
            }

            // B-R-007: Attachments passthrough - echo all attachments as key=value pairs
            if ("echoAttachments".equals(methodName)) {
                if (attachments.isEmpty()) {
                    return "no-attachments";
                }
                StringBuilder sb = new StringBuilder();
                for (Map.Entry<String, String> entry : attachments.entrySet()) {
                    if (sb.length() > 0) sb.append(",");
                    sb.append(entry.getKey()).append("=").append(entry.getValue());
                }
                return sb.toString();
            }

            return "mock:" + methodName;
        }

        private byte[] encodeErrorResponse(String errorMsg) {
            try {
                ByteArrayOutputStream baos = new ByteArrayOutputStream();
                Hessian2Output h2o = new Hessian2Output(baos);
                h2o.writeInt(0); // RESPONSE_WITH_EXCEPTION
                h2o.writeString(errorMsg);
                h2o.flush();
                return baos.toByteArray();
            } catch (IOException e) {
                return new byte[]{'N'};
            }
        }

        @Override
        public void exceptionCaught(ChannelHandlerContext ctx, Throwable cause) {
            System.err.println("[DubboTcpTestServer] Error: " + cause.getMessage());
            ctx.close();
        }
    }

    // ── Dubbo message container ──────────────────────────────────────────

    static class DubboMessage {
        byte flags;
        byte status;
        long requestId;
        boolean isRequest;
        boolean isTwoWay;
        boolean isEvent;
        byte serialId;
        byte[] body;
    }

    // ── Main entry point for standalone testing ──────────────────────────

    public static void main(String[] args) throws InterruptedException {
        int port = args.length > 0 ? Integer.parseInt(args[0]) : 20880;
        DubboTcpTestServer server = new DubboTcpTestServer(port);

        // Register shutdown hook
        Runtime.getRuntime().addShutdownHook(new Thread(server::shutdown));

        server.start();
        System.out.println("[DubboTcpTestServer] Press Ctrl+C to stop");

        // Block main thread
        Thread.currentThread().join();
    }
}
