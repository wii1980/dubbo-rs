pub use dubbo_rs_common;
pub use dubbo_rs_registry;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use dashmap::DashMap;
use dubbo_rs_common::error::RPCError;
use dubbo_rs_common::node::Node;
use dubbo_rs_common::url::URL;
use dubbo_rs_registry::{NotifyListener, Registry, ServiceEvent};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const DEFAULT_NACOS_GROUP: &str = "DEFAULT_GROUP";
const DEFAULT_NAMESPACE: &str = "public";
const DEFAULT_WEIGHT: f64 = 1.0;
const HEARTBEAT_INTERVAL_SECS: u64 = 5;
const POLL_INTERVAL_SECS: u64 = 10;

#[allow(clippy::missing_errors_doc)]
pub fn check_nacos_response(text: &str) -> Result<(), String> {
    // Nacos 1.x returns plain "ok"
    if text == "ok" {
        return Ok(());
    }
    let body = serde_json::from_str::<Value>(text).map_err(|e| format!("parse failed: {e}"))?;
    match body.get("code").and_then(serde_json::Value::as_i64) {
        Some(code) if code == 200 || code == 0 => Ok(()),
        Some(code) => {
            let msg = body
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown");
            Err(format!("code={code}, message={msg}"))
        }
        None => Ok(()),
    }
}

#[must_use]
pub fn extract_hosts(text: &str) -> Option<Vec<NacosInstance>> {
    let body: Value = serde_json::from_str(text).ok()?;
    let raw_hosts = body
        .get("hosts")
        .or_else(|| body.get("data").and_then(|d| d.get("hosts")));
    raw_hosts.and_then(|h| serde_json::from_value(h.clone()).ok())
}

#[derive(Debug, Serialize, Clone)]
pub struct InstanceRegisterRequest {
    #[serde(rename = "serviceName")]
    pub service_name: String,
    pub ip: String,
    pub port: u32,
    #[serde(rename = "namespaceId")]
    pub namespace_id: String,
    pub weight: f64,
    pub healthy: bool,
    pub enabled: bool,
    pub ephemeral: bool,
    #[serde(rename = "groupName")]
    pub group_name: String,
    pub metadata: String,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct NacosInstance {
    pub ip: String,
    pub port: u32,
    pub weight: f64,
    pub healthy: bool,
    #[serde(rename = "serviceName")]
    pub service_name: String,
    pub metadata: Option<HashMap<String, String>>,
}

#[derive(Debug, Serialize, Clone)]
struct HeartbeatRequest {
    #[serde(rename = "serviceName")]
    service_name: String,
    ip: String,
    port: u32,
    #[serde(rename = "namespaceId")]
    namespace_id: String,
    #[serde(rename = "groupName")]
    group_name: String,
    ephemeral: bool,
}

pub struct NacosRegistry {
    url: URL,
    pub server_addr: String,
    pub namespace: String,
    pub group: String,
    pub username: Option<String>,
    pub password: Option<String>,
    client: Client,
    listeners: DashMap<String, Vec<Arc<dyn NotifyListener>>>,
    heartbeat_handles: RwLock<Vec<tokio::task::JoinHandle<()>>>,
    poll_handles: RwLock<Vec<tokio::task::JoinHandle<()>>>,
}

impl NacosRegistry {
    #[must_use]
    pub fn new(url: URL) -> Self {
        let server_addr = format!("http://{}:{}", url.ip, url.port);
        Self {
            url,
            server_addr,
            namespace: DEFAULT_NAMESPACE.to_string(),
            group: DEFAULT_NACOS_GROUP.to_string(),
            username: None,
            password: None,
            client: Client::new(),
            listeners: DashMap::new(),
            heartbeat_handles: RwLock::new(Vec::new()),
            poll_handles: RwLock::new(Vec::new()),
        }
    }

    #[must_use]
    pub fn with_namespace(mut self, ns: impl Into<String>) -> Self {
        self.namespace = ns.into();
        self
    }

    #[must_use]
    pub fn with_group(mut self, group: impl Into<String>) -> Self {
        self.group = group.into();
        self
    }

    #[must_use]
    pub fn with_auth(mut self, username: impl Into<String>, password: impl Into<String>) -> Self {
        self.username = Some(username.into());
        self.password = Some(password.into());
        self
    }

    pub fn build_register_request(&self, service_url: &URL) -> InstanceRegisterRequest {
        let port: u32 = service_url.port.parse().unwrap_or(0);
        InstanceRegisterRequest {
            service_name: service_url.path.trim_start_matches('/').to_string(),
            ip: service_url.ip.clone(),
            port,
            namespace_id: self.namespace.clone(),
            weight: DEFAULT_WEIGHT,
            healthy: true,
            enabled: true,
            ephemeral: true,
            group_name: self.group.clone(),
            metadata: "{}".to_string(),
        }
    }

    async fn do_register(&self, req: &InstanceRegisterRequest) -> Result<(), RPCError> {
        let url = format!("{}/nacos/v1/ns/instance", self.server_addr);
        let mut builder = self.client.post(&url).query(&[
            ("serviceName", req.service_name.as_str()),
            ("ip", req.ip.as_str()),
            ("port", &req.port.to_string()),
            ("namespaceId", req.namespace_id.as_str()),
            ("weight", &req.weight.to_string()),
            ("healthy", &req.healthy.to_string()),
            ("enabled", &req.enabled.to_string()),
            ("ephemeral", &req.ephemeral.to_string()),
            ("groupName", req.group_name.as_str()),
            ("metadata", req.metadata.as_str()),
        ]);

        if let (Some(u), Some(p)) = (&self.username, &self.password) {
            builder = builder.query(&[("accessKey", u.as_str()), ("secretKey", p.as_str())]);
        }

        let resp = builder
            .send()
            .await
            .map_err(|e| RPCError::ServerError(format!("Nacos register failed: {e}")))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| RPCError::ServerError(format!("Nacos register read failed: {e}")))?;

        if !status.is_success() {
            return Err(RPCError::ServerError(format!(
                "Nacos register HTTP {status}: {text}",
            )));
        }

        if let Err(msg) = check_nacos_response(&text) {
            return Err(RPCError::ServerError(format!(
                "Nacos register rejected: {msg} (body: {text})"
            )));
        }

        tracing::info!(
            "Nacos instance registered: service={}, ip={}:{}, namespace={}, group={}",
            req.service_name,
            req.ip,
            req.port,
            req.namespace_id,
            req.group_name
        );
        Ok(())
    }

    async fn do_deregister(&self, req: &InstanceRegisterRequest) -> Result<(), RPCError> {
        let url = format!("{}/nacos/v1/ns/instance", self.server_addr);
        let mut builder = self.client.delete(&url).query(&[
            ("serviceName", req.service_name.as_str()),
            ("ip", req.ip.as_str()),
            ("port", &req.port.to_string()),
            ("namespaceId", req.namespace_id.as_str()),
            ("groupName", req.group_name.as_str()),
            ("ephemeral", &req.ephemeral.to_string()),
        ]);

        if let (Some(u), Some(p)) = (&self.username, &self.password) {
            builder = builder.query(&[("accessKey", u.as_str()), ("secretKey", p.as_str())]);
        }

        let resp = builder
            .send()
            .await
            .map_err(|e| RPCError::ServerError(format!("Nacos deregister failed: {e}")))?;

        let status = resp.status();
        let text = resp
            .text()
            .await
            .map_err(|e| RPCError::ServerError(format!("Nacos deregister read failed: {e}")))?;

        if !status.is_success() {
            return Err(RPCError::ServerError(format!(
                "Nacos deregister HTTP {status}: {text}"
            )));
        }

        if let Err(msg) = check_nacos_response(&text) {
            return Err(RPCError::ServerError(format!(
                "Nacos deregister rejected: {msg} (body: {text})"
            )));
        }

        Ok(())
    }

    #[allow(dead_code)]
    async fn send_heartbeat(&self, req: &HeartbeatRequest) -> Result<(), RPCError> {
        let url = format!("{}/nacos/v1/ns/instance/beat", self.server_addr);
        let mut builder = self.client.put(&url).query(&[
            ("serviceName", req.service_name.as_str()),
            ("ip", req.ip.as_str()),
            ("port", &req.port.to_string()),
            ("namespaceId", req.namespace_id.as_str()),
            ("groupName", req.group_name.as_str()),
            ("ephemeral", &req.ephemeral.to_string()),
        ]);

        if let (Some(u), Some(p)) = (&self.username, &self.password) {
            builder = builder.query(&[("accessKey", u.as_str()), ("secretKey", p.as_str())]);
        }

        let _resp = builder
            .send()
            .await
            .map_err(|e| RPCError::ServerError(format!("Nacos heartbeat failed: {e}")))?;

        Ok(())
    }

    #[allow(dead_code)]
    async fn discover_instances(&self, service_name: &str) -> Result<Vec<URL>, RPCError> {
        let url = format!("{}/nacos/v1/ns/instance/list", self.server_addr);
        let mut builder = self.client.get(&url).query(&[
            ("serviceName", service_name),
            ("namespaceId", self.namespace.as_str()),
            ("groupName", self.group.as_str()),
            ("healthyOnly", "true"),
        ]);

        if let (Some(u), Some(p)) = (&self.username, &self.password) {
            builder = builder.query(&[("accessKey", u.as_str()), ("secretKey", p.as_str())]);
        }

        let resp = builder
            .send()
            .await
            .map_err(|e| RPCError::ServerError(format!("Nacos discover failed: {e}")))?;

        let text = resp
            .text()
            .await
            .map_err(|e| RPCError::ServerError(format!("Nacos discover read failed: {e}")))?;

        check_nacos_response(&text).map_err(|msg| {
            RPCError::ServerError(format!("Nacos discover error: {msg} (body: {text})"))
        })?;

        let hosts = extract_hosts(&text).unwrap_or_default();
        let urls: Vec<URL> = hosts
            .iter()
            .map(|inst| {
                let mut u = URL::new("dubbo", format!("/{}", inst.service_name.clone()));
                u.ip.clone_from(&inst.ip);
                u.port = inst.port.to_string();
                u.set_param("weight", inst.weight.to_string());
                u
            })
            .collect();

        Ok(urls)
    }
}

impl Node for NacosRegistry {
    fn get_url(&self) -> &URL {
        &self.url
    }

    fn is_available(&self) -> bool {
        true
    }

    fn destroy(&self) {
        for handle in self.heartbeat_handles.read().unwrap().iter() {
            handle.abort();
        }
        for handle in self.poll_handles.read().unwrap().iter() {
            handle.abort();
        }
    }
}

#[async_trait]
impl Registry for NacosRegistry {
    async fn register(&self, url: URL) -> Result<(), RPCError> {
        let req = self.build_register_request(&url);
        self.do_register(&req).await?;

        let heartbeat_req = HeartbeatRequest {
            service_name: req.service_name.clone(),
            ip: req.ip.clone(),
            port: req.port,
            namespace_id: req.namespace_id.clone(),
            group_name: req.group_name.clone(),
            ephemeral: true,
        };

        let client = self.client.clone();
        let server_addr = self.server_addr.clone();
        let username = self.username.clone();
        let password = self.password.clone();

        let handle = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(HEARTBEAT_INTERVAL_SECS)).await;
                let url = format!("{server_addr}/nacos/v1/ns/instance/beat");
                let mut builder = client.put(&url).query(&[
                    ("serviceName", heartbeat_req.service_name.as_str()),
                    ("ip", heartbeat_req.ip.as_str()),
                    ("port", &heartbeat_req.port.to_string()),
                    ("namespaceId", heartbeat_req.namespace_id.as_str()),
                    ("groupName", heartbeat_req.group_name.as_str()),
                    ("ephemeral", "true"),
                ]);
                if let (Some(u), Some(p)) = (&username, &password) {
                    builder =
                        builder.query(&[("accessKey", u.as_str()), ("secretKey", p.as_str())]);
                }
                let _ = builder.send().await;
            }
        });

        self.heartbeat_handles.write().unwrap().push(handle);
        Ok(())
    }

    async fn unregister(&self, url: URL) -> Result<(), RPCError> {
        let req = self.build_register_request(&url);
        self.do_deregister(&req).await
    }

    async fn subscribe(&self, url: URL, listener: Arc<dyn NotifyListener>) -> Result<(), RPCError> {
        let service_name = url.path.trim_start_matches('/').to_string();
        let service_key = service_name.clone();

        self.listeners
            .entry(service_key.clone())
            .or_default()
            .push(listener);

        let client = self.client.clone();
        let server_addr = self.server_addr.clone();
        let namespace = self.namespace.clone();
        let group = self.group.clone();
        let username = self.username.clone();
        let password = self.password.clone();
        let listeners = self.listeners.clone();

        let handle = tokio::spawn(async move {
            let mut previous_hosts: Vec<String> = Vec::new();

            loop {
                let url = format!("{server_addr}/nacos/v1/ns/instance/list");
                let mut builder = client.get(&url).query(&[
                    ("serviceName", service_name.as_str()),
                    ("namespaceId", namespace.as_str()),
                    ("groupName", group.as_str()),
                    ("healthyOnly", "true"),
                ]);

                if let (Some(u), Some(p)) = (&username, &password) {
                    builder =
                        builder.query(&[("accessKey", u.as_str()), ("secretKey", p.as_str())]);
                }

                let resp = match builder.send().await {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::warn!("Nacos poll failed for service '{service_name}': {e}");
                        tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
                        continue;
                    }
                };

                let text = match resp.text().await {
                    Ok(t) => t,
                    Err(e) => {
                        tracing::warn!("Nacos poll read failed for service '{service_name}': {e}");
                        tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
                        continue;
                    }
                };

                if let Err(msg) = check_nacos_response(&text) {
                    tracing::warn!(
                        "Nacos poll error for service '{service_name}': {msg} (body: {text})"
                    );
                    tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
                    continue;
                }

                let hosts = extract_hosts(&text).unwrap_or_default();

                let current: Vec<String> = hosts
                    .iter()
                    .map(|h| format!("{}:{}", h.ip, h.port))
                    .collect();

                if current != previous_hosts {
                    let urls: Vec<URL> = hosts
                        .iter()
                        .map(|h| {
                            let mut u = URL::new("dubbo", format!("/{}", h.service_name));
                            u.ip.clone_from(&h.ip);
                            u.port = h.port.to_string();
                            u
                        })
                        .collect();

                    tracing::info!(
                        "Nacos service '{}' discovered {} instance(s)",
                        service_name,
                        urls.len()
                    );
                    if let Some(entry) = listeners.get(&service_key) {
                        for listener in entry.value() {
                            listener.notify(ServiceEvent::Update(urls.clone())).await;
                        }
                    }
                    previous_hosts = current;
                }

                tokio::time::sleep(Duration::from_secs(POLL_INTERVAL_SECS)).await;
            }
        });

        self.poll_handles.write().unwrap().push(handle);
        Ok(())
    }

    async fn unsubscribe(
        &self,
        url: URL,
        _listener: Arc<dyn NotifyListener>,
    ) -> Result<(), RPCError> {
        let service_key = url.path.trim_start_matches('/').to_string();
        self.listeners.remove(&service_key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_nacos_url() -> URL {
        let mut url = URL::new("nacos", "");
        url.ip = "127.0.0.1".to_string();
        url.port = "8848".to_string();
        url
    }

    #[test]
    fn test_nacos_registry_creation() {
        let registry = NacosRegistry::new(make_nacos_url());
        assert!(registry.is_available());
        assert_eq!(registry.server_addr, "http://127.0.0.1:8848");
    }

    #[test]
    fn test_nacos_with_namespace() {
        let registry = NacosRegistry::new(make_nacos_url()).with_namespace("dev");
        assert_eq!(registry.namespace, "dev");
    }

    #[test]
    fn test_nacos_with_group() {
        let registry = NacosRegistry::new(make_nacos_url()).with_group("MY_GROUP");
        assert_eq!(registry.group, "MY_GROUP");
    }

    #[test]
    fn test_nacos_with_auth() {
        let registry = NacosRegistry::new(make_nacos_url()).with_auth("admin", "secret123");
        assert_eq!(registry.username, Some("admin".to_string()));
        assert_eq!(registry.password, Some("secret123".to_string()));
    }

    #[test]
    fn test_nacos_default_group() {
        let registry = NacosRegistry::new(make_nacos_url());
        assert_eq!(registry.group, DEFAULT_NACOS_GROUP);
    }

    #[test]
    fn test_nacos_default_namespace() {
        let registry = NacosRegistry::new(make_nacos_url());
        assert_eq!(registry.namespace, DEFAULT_NAMESPACE);
    }

    #[test]
    fn test_build_register_request() {
        let registry = NacosRegistry::new(make_nacos_url());
        let mut svc_url = URL::new("tri", "/com.example.GreetService");
        svc_url.ip = "10.0.0.1".to_string();
        svc_url.port = "20880".to_string();

        let req = registry.build_register_request(&svc_url);
        assert_eq!(req.service_name, "com.example.GreetService");
        assert_eq!(req.ip, "10.0.0.1");
        assert_eq!(req.port, 20880);
        assert_eq!(req.namespace_id, DEFAULT_NAMESPACE);
        assert!(req.healthy);
        assert!(req.enabled);
        assert!(req.ephemeral);
    }

    #[test]
    fn test_build_register_request_with_namespace() {
        let registry = NacosRegistry::new(make_nacos_url()).with_namespace("prod");
        let mut svc_url = URL::new("tri", "/com.example.GreetService");
        svc_url.ip = "10.0.0.1".to_string();
        svc_url.port = "20880".to_string();

        let req = registry.build_register_request(&svc_url);
        assert_eq!(req.namespace_id, "prod");
    }

    #[test]
    fn test_nacos_url_without_proto_prefix() {
        let mut url = URL::new("nacos", "");
        url.ip = "nacos-server".to_string();
        url.port = "8848".to_string();
        let registry = NacosRegistry::new(url);
        assert!(registry.server_addr.contains("nacos-server"));
    }
}
