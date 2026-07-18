# Dubbo 三生态对比完善计划

## 目标

完善 dubbo-java、dubbo-go、dubbo-rs 三者使用方法对比文档，补充以下四个方面的内容：

1. 配置风格 YAML 对比
2. 注册中心增加 Redis 支持
3. 协议补全 Dubbo 协议
4. 泛化调用优化

---

## 待完善内容

### 1. 配置风格 YAML 对比

**目标**：补充三个项目各自的 YAML 配置文件示例和配置项说明

| 项目 | 配置方式 | 说明 |
|------|----------|------|
| dubbo-java | Spring Boot `application.yml` / `dubbo.properties` | 展示完整的 Dubbo 配置结构 |
| dubbo-go | YAML 文件 + `config.Load()` | 展示配置加载方式 |
| dubbo-rs | 纯代码 Builder（无 YAML） | 说明 Rust 生态不常用 YAML 的特点 |

**需要补充的内容**：
- Java: application.yml 示例
- Java: @DubboReference 注解配置属性
- Go: YAML 配置文件示例
- Go: config.Load() 加载方式
- Rust: 说明为何使用代码配置（Dubbo3 生态特点）

---

### 2. 注册中心增加 Redis

**目标**：补充三个项目对 Redis 作为注册中心的支持情况

| 项目 | Redis 支持 | 配置方式 |
|------|------------|----------|
| dubbo-java | ✅ 支持 | `redis://127.0.0.1:6379` |
| dubbo-go | ✅ 支持 | `registry.WithRedis()` |
| dubbo-rs | ❌ 暂不支持 | 说明当前支持的注册中心 |

**需要补充的内容**：
- Java: Redis 注册中心配置示例
- Go: Redis 注册中心配置示例
- Rust: 当前支持的注册中心列表（Zookeeper / Nacos / etcd）
- 说明 Rust 暂不支持 Redis 的现状

---

### 3. 协议补全 Dubbo 协议

**目标**：补充 Dubbo2 二进制协议（非 Triple）在三个项目中的支持情况

| 项目 | Dubbo2 协议 | 说明 |
|------|-------------|------|
| dubbo-java | ✅ 原生支持 | 默认协议，成熟稳定 |
| dubbo-go | ✅ 支持 | `protocol.WithDubbo()` |
| dubbo-rs | ❌ 不支持 | 专注 Triple 协议，说明原因 |

**需要补充的内容**：
- Java: Dubbo 协议配置示例
- Go: Dubbo 协议配置示例
- Rust: 说明仅支持 Triple 协议，不支持 Dubbo2
- 解释 Rust 选择仅支持 Triple 的技术原因

---

### 4. 泛化调用优化

**目标**：优化泛化调用部分的对比内容，提供更详细的示例

| 项目 | 泛化调用方式 | 示例完整性 |
|------|--------------|------------|
| dubbo-java | `ReferenceConfig.setGeneric(true)` + `GenericService.$invoke` | ⭐⭐⭐ 完整 |
| dubbo-go | `cli.NewGenericService()` + `genericService.Invoke()` | ⭐⭐ 需补充 |
| dubbo-rs | 不支持 | ⭐ 需说明 |

**需要补充的内容**：
- Java: 完整的泛化调用示例代码
- Go: 补充完整的泛化调用示例（高级 API + 低级 API）
- Rust: 明确说明不支持泛化调用的原因
- 提供三个项目的泛化调用对比表格

---

## 输出形式

完善后的对比文档将以 Markdown 格式呈现，包含：

1. 新的对比表格（YAML / 注册中心 / 协议 / 泛化调用）
2. 各项目的配置示例代码块
3. 差异总结

---

## 预期产出

- 计划确认后，更新现有对比文档
- 补充约 30-40 行新内容
- 提供可运行的配置示例代码