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

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.Serializable;
import java.math.BigDecimal;
import java.math.BigInteger;
import java.util.ArrayList;
import java.util.Date;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;

import com.alibaba.com.caucho.hessian.io.Hessian2Output;

/**
 * Generates Hessian2 binary fixtures from Java objects.
 *
 * <p>Uses hessian-lite (the same library dubbo-java uses internally) to produce
 * reference binary files. These files are then decoded by dubbo-rs's
 * {@code dubbo-serialization-hessian2} crate to verify cross-language compatibility.</p>
 *
 * <p>Usage: {@code mvn compile exec:java} or run main directly.</p>
 *
 * <p>Output directory: {@code target/fixtures/}</p>
 */
public class Hessian2ReferenceEncoder {

    private static final String OUTPUT_DIR = "target/fixtures";

    // ── Entry point ──────────────────────────────────────────────────────

    public static void main(String[] args) throws Exception {
        String outputDir = args.length > 0 ? args[0] : OUTPUT_DIR;
        File dir = new File(outputDir);
        if (!dir.exists() && !dir.mkdirs()) {
            throw new IOException("Failed to create output directory: " + outputDir);
        }

        encodePrimitives(dir);
        encodeStrings(dir);
        encodeCollections(dir);
        encodeMaps(dir);
        encodeDates(dir);
        encodePojo(dir);
        encodeNull(dir);

        System.out.println("All Hessian2 fixtures written to " + dir.getAbsolutePath());
    }

    // ── Primitives ───────────────────────────────────────────────────────

    private static void encodePrimitives(File dir) throws IOException {
        writeFixture(dir, "bool_true.bin", out -> out.writeBoolean(true));
        writeFixture(dir, "bool_false.bin", out -> out.writeBoolean(false));
        writeFixture(dir, "int_zero.bin", out -> out.writeInt(0));
        writeFixture(dir, "int_one.bin", out -> out.writeInt(1));
        writeFixture(dir, "int_47.bin", out -> out.writeInt(47));          // compact int max
        writeFixture(dir, "int_neg16.bin", out -> out.writeInt(-16));      // compact int min
        writeFixture(dir, "int_256.bin", out -> out.writeInt(256));
        writeFixture(dir, "int_max.bin", out -> out.writeInt(Integer.MAX_VALUE));
        writeFixture(dir, "int_min.bin", out -> out.writeInt(Integer.MIN_VALUE));
        writeFixture(dir, "long_zero.bin", out -> out.writeLong(0L));
        writeFixture(dir, "long_15.bin", out -> out.writeLong(15L));       // compact long max
        writeFixture(dir, "long_neg8.bin", out -> out.writeLong(-8L));     // compact long min
        writeFixture(dir, "long_big.bin", out -> out.writeLong(Long.MAX_VALUE));
        writeFixture(dir, "double_zero.bin", out -> out.writeDouble(0.0));
        writeFixture(dir, "double_one.bin", out -> out.writeDouble(1.0));
        writeFixture(dir, "double_pi.bin", out -> out.writeDouble(3.14159));
        writeFixture(dir, "double_neg.bin", out -> out.writeDouble(-99.99));
        writeFixture(dir, "float_as_double.bin", out -> out.writeDouble(1.5f));
    }

    // ── Strings ──────────────────────────────────────────────────────────

    private static void encodeStrings(File dir) throws IOException {
        writeFixture(dir, "string_empty.bin", out -> out.writeString(""));
        writeFixture(dir, "string_ascii.bin", out -> out.writeString("hello"));
        writeFixture(dir, "string_unicode.bin", out -> out.writeString("你好世界"));
        writeFixture(dir, "string_mixed.bin", out -> out.writeString("Hello, 世界! 🦀"));
        writeFixture(dir, "string_long.bin",
                out -> out.writeString("a".repeat(1024)));
        // String that exceeds single-chunk length (>= 65536 chars triggers chunked encoding)
        writeFixture(dir, "string_chunked.bin",
                out -> out.writeString("x".repeat(100_000)));
    }

    // ── Collections ──────────────────────────────────────────────────────

    private static void encodeCollections(File dir) throws IOException {
        // Empty list
        writeFixture(dir, "list_empty.bin",
                out -> out.writeObject(new ArrayList<>()));

        // Typed list of integers
        List<Integer> intList = new ArrayList<>();
        intList.add(1);
        intList.add(2);
        intList.add(3);
        writeFixture(dir, "list_int.bin", out -> out.writeObject(intList));

        // Untyped list (mixed types)
        List<Object> mixedList = new ArrayList<>();
        mixedList.add("hello");
        mixedList.add(42);
        mixedList.add(true);
        mixedList.add(null);
        writeFixture(dir, "list_mixed.bin", out -> out.writeObject(mixedList));

        // Nested list
        List<List<String>> nestedList = new ArrayList<>();
        List<String> inner = new ArrayList<>();
        inner.add("a");
        inner.add("b");
        nestedList.add(inner);
        nestedList.add(new ArrayList<>());
        writeFixture(dir, "list_nested.bin", out -> out.writeObject(nestedList));
    }

    // ── Maps ─────────────────────────────────────────────────────────────

    private static void encodeMaps(File dir) throws IOException {
        // Empty map
        writeFixture(dir, "map_empty.bin",
                out -> out.writeObject(new HashMap<>()));

        // String-to-string map
        Map<String, String> strMap = new LinkedHashMap<>();
        strMap.put("key1", "value1");
        strMap.put("key2", "value2");
        writeFixture(dir, "map_string.bin", out -> out.writeObject(strMap));

        // Mixed-type map
        Map<String, Object> mixedMap = new LinkedHashMap<>();
        mixedMap.put("name", "dubbo-rs");
        mixedMap.put("version", 3);
        mixedMap.put("stable", true);
        mixedMap.put("ratio", 0.95);
        mixedMap.put("nullable", null);
        writeFixture(dir, "map_mixed.bin", out -> out.writeObject(mixedMap));
    }

    // ── Dates ────────────────────────────────────────────────────────────

    private static void encodeDates(File dir) throws IOException {
        // Unix epoch
        writeFixture(dir, "date_epoch.bin",
                out -> out.writeObject(new Date(0L)));

        // A fixed timestamp: 2024-01-15T10:30:00 UTC
        writeFixture(dir, "date_fixed.bin",
                out -> out.writeObject(new Date(1705312200000L)));
    }

    // ── POJOs ────────────────────────────────────────────────────────────

    private static void encodePojo(File dir) throws IOException {
        // Simple POJO
        SamplePerson person = new SamplePerson();
        person.setName("Alice");
        person.setAge(30);
        person.setActive(true);
        writeFixture(dir, "pojo_person.bin", out -> out.writeObject(person));

        // POJO with nested POJO
        SampleOrder order = new SampleOrder();
        order.setOrderId("ORD-001");
        order.setCustomer(person);
        order.setAmount(new BigDecimal("99.95"));
        order.setItems(List.of("item1", "item2", "item3"));
        writeFixture(dir, "pojo_order.bin", out -> out.writeObject(order));

        // POJO with null fields
        SamplePerson sparse = new SamplePerson();
        sparse.setName(null);
        sparse.setAge(0);
        sparse.setActive(false);
        writeFixture(dir, "pojo_sparse.bin", out -> out.writeObject(sparse));
    }

    // ── Null ─────────────────────────────────────────────────────────────

    private static void encodeNull(File dir) throws IOException {
        writeFixture(dir, "null.bin", out -> out.writeObject(null));
    }

    // ── Helpers ──────────────────────────────────────────────────────────

    @FunctionalInterface
    private interface HessianWriter {
        void write(Hessian2Output out) throws IOException;
    }

    private static void writeFixture(File dir, String filename, HessianWriter writer)
            throws IOException {
        File file = new File(dir, filename);
        try (FileOutputStream fos = new FileOutputStream(file)) {
            Hessian2Output h2o = new Hessian2Output(fos);
            writer.write(h2o);
            h2o.flushBuffer();
        }
        System.out.println("  Written: " + filename);
    }

    // ── Sample POJOs ─────────────────────────────────────────────────────

    /**
     * Simple person POJO for fixture generation.
     * Field names must match what dubbo-rs Hessian2 decoder expects.
     */
    public static class SamplePerson implements Serializable {
        private static final long serialVersionUID = 1L;

        private String name;
        private int age;
        private boolean active;

        public String getName() { return name; }
        public void setName(String name) { this.name = name; }

        public int getAge() { return age; }
        public void setAge(int age) { this.age = age; }

        public boolean isActive() { return active; }
        public void setActive(boolean active) { this.active = active; }

        @Override
        public boolean equals(Object o) {
            if (this == o) return true;
            if (!(o instanceof SamplePerson that)) return false;
            return age == that.age && active == that.active && Objects.equals(name, that.name);
        }

        @Override
        public int hashCode() {
            return Objects.hash(name, age, active);
        }

        @Override
        public String toString() {
            return "SamplePerson{name='" + name + "', age=" + age + ", active=" + active + "}";
        }
    }

    /**
     * Order POJO with nested objects for complex fixture generation.
     */
    public static class SampleOrder implements Serializable {
        private static final long serialVersionUID = 1L;

        private String orderId;
        private SamplePerson customer;
        private BigDecimal amount;
        private List<String> items;

        public String getOrderId() { return orderId; }
        public void setOrderId(String orderId) { this.orderId = orderId; }

        public SamplePerson getCustomer() { return customer; }
        public void setCustomer(SamplePerson customer) { this.customer = customer; }

        public BigDecimal getAmount() { return amount; }
        public void setAmount(BigDecimal amount) { this.amount = amount; }

        public List<String> getItems() { return items; }
        public void setItems(List<String> items) { this.items = items; }

        @Override
        public String toString() {
            return "SampleOrder{orderId='" + orderId + "', customer=" + customer
                    + ", amount=" + amount + ", items=" + items + "}";
        }
    }
}
