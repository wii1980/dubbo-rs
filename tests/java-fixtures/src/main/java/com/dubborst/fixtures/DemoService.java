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

/**
 * Demo service interface for cross-language verification.
 * Mirrors the service used in dubbo-rs examples.
 */
public interface DemoService {

    /**
     * Simple greeting method.
     *
     * @param name the name to greet
     * @return greeting message
     */
    String sayHello(String name);
}
