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

import org.apache.dubbo.config.ApplicationConfig;
import org.apache.dubbo.config.ProtocolConfig;
import org.apache.dubbo.config.RegistryConfig;
import org.apache.dubbo.config.ServiceConfig;

public class RegistryTestProvider {

    public static void main(String[] args) throws InterruptedException {
        if (args.length < 2) {
            System.err.println("Usage: RegistryTestProvider <registry-type> <registry-address> [port]");
            System.exit(1);
        }

        String registryType = args[0];
        String registryAddress = args[1];
        int port = args.length > 2 ? Integer.parseInt(args[2]) : 20880;

        ApplicationConfig application = new ApplicationConfig("java-cross-lang-provider");

        String registryUrl = registryType + "://" + registryAddress;
        RegistryConfig registry = new RegistryConfig(registryUrl);

        ProtocolConfig protocol = new ProtocolConfig("dubbo", port);

        ServiceConfig<DemoService> service = new ServiceConfig<>();
        service.setApplication(application);
        service.setRegistry(registry);
        service.setProtocol(protocol);
        service.setInterface(DemoService.class);
        service.setRef(new DemoServiceImpl());
        service.setGroup("cross-lang");
        service.setVersion("1.0.0");

        service.export();

        System.out.println("[RegistryTestProvider] Exported DemoService to " + registryUrl + " on port " + port);
        System.out.println("[RegistryTestProvider] Service registered. Press Ctrl+C to stop.");

        Thread.currentThread().join();
    }
}
