use std::collections::HashMap;
use std::fmt::Write;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct URL {
    pub protocol: String,
    pub location: String,
    pub ip: String,
    pub port: String,
    pub path: String,
    pub username: String,
    pub password: String,
    pub methods: Vec<String>,
    pub params: HashMap<String, String>,
}

impl URL {
    pub fn new(protocol: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            protocol: protocol.into(),
            path: path.into(),
            ..Default::default()
        }
    }

    #[must_use]
    pub fn get_param(&self, key: &str) -> Option<&String> {
        self.params.get(key)
    }

    #[must_use]
    pub fn get_param_or_default(&self, key: &str, default: &str) -> String {
        self.get_param(key)
            .cloned()
            .unwrap_or_else(|| default.to_string())
    }

    pub fn set_param(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.params.insert(key.into(), value.into());
    }

    #[must_use]
    pub fn get_method_param(&self, method: &str, key: &str) -> Option<&String> {
        let method_key = format!("{method}.{key}");
        self.params.get(&method_key)
    }

    #[must_use]
    pub fn get_service_key(&self) -> String {
        format!("{}/{}", self.path, self.get_group())
    }

    #[must_use]
    pub fn get_address(&self) -> String {
        format!("{}:{}", self.ip, self.port)
    }

    #[must_use]
    pub fn get_group(&self) -> String {
        self.get_param_or_default("group", "")
    }

    #[must_use]
    pub fn get_version(&self) -> String {
        self.get_param_or_default("version", "1.0.0")
    }

    #[must_use]
    pub fn to_full_string(&self) -> String {
        let mut params_str = String::new();
        for (i, (k, v)) in self.params.iter().enumerate() {
            if i > 0 {
                params_str.push('&');
            }
            let _ = write!(params_str, "{k}={v}");
        }
        format!(
            "{}://{}:{}/{}/{}?{}",
            self.protocol,
            self.ip,
            self.port,
            self.path,
            self.get_version(),
            params_str
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_default() {
        let url = URL::default();
        assert!(url.protocol.is_empty());
        assert!(url.params.is_empty());
    }

    #[test]
    fn test_url_new() {
        let url = URL::new("dubbo", "/com.example.DemoService");
        assert_eq!(url.protocol, "dubbo");
        assert_eq!(url.path, "/com.example.DemoService");
    }

    #[test]
    fn test_url_params() {
        let mut url = URL::new("dubbo", "/com.example.DemoService");
        url.set_param("version", "1.0.0");
        assert_eq!(url.get_param("version"), Some(&"1.0.0".to_string()));
        assert_eq!(url.get_version(), "1.0.0");
    }

    #[test]
    fn test_url_service_key() {
        let mut url = URL::new("dubbo", "/com.example.DemoService");
        url.set_param("group", "test");
        assert_eq!(url.get_service_key(), "/com.example.DemoService/test");
    }

    #[test]
    fn test_url_full_string() {
        let mut url = URL::new("dubbo", "/com.example.DemoService");
        url.ip = "192.168.1.1".to_string();
        url.port = "20880".to_string();
        url.set_param("version", "1.0.0");
        url.set_param("application", "demo");
        let full = url.to_full_string();
        assert!(full.starts_with("dubbo://192.168.1.1:20880"));
        assert!(full.contains("version=1.0.0"));
        assert!(full.contains("application=demo"));
    }

    #[test]
    fn test_get_method_param() {
        let mut url = URL::new("tri", "/test");
        url.set_param("sayHello.timeout", "3000");
        assert_eq!(
            url.get_method_param("sayHello", "timeout"),
            Some(&"3000".to_string())
        );
    }

    #[test]
    fn test_get_method_param_not_found() {
        let url = URL::new("tri", "/test");
        assert_eq!(url.get_method_param("unknown", "key"), None);
    }

    #[test]
    fn test_get_address() {
        let mut url = URL::new("tri", "/test");
        url.ip = "10.0.0.1".to_string();
        url.port = "20880".to_string();
        assert_eq!(url.get_address(), "10.0.0.1:20880");
    }

    #[test]
    fn test_get_address_empty() {
        let url = URL::default();
        assert_eq!(url.get_address(), ":");
    }

    #[test]
    fn test_get_group_default() {
        let url = URL::new("tri", "/test");
        assert_eq!(url.get_group(), "");
    }

    #[test]
    fn test_get_group_custom() {
        let mut url = URL::new("tri", "/test");
        url.set_param("group", "mygroup");
        assert_eq!(url.get_group(), "mygroup");
    }

    #[test]
    fn test_get_version_default() {
        let url = URL::new("tri", "/test");
        assert_eq!(url.get_version(), "1.0.0");
    }

    #[test]
    fn test_get_version_custom() {
        let mut url = URL::new("tri", "/test");
        url.set_param("version", "2.0.0");
        assert_eq!(url.get_version(), "2.0.0");
    }

    #[test]
    fn test_set_param_overwrite() {
        let mut url = URL::new("tri", "/test");
        url.set_param("timeout", "1000");
        assert_eq!(url.get_param("timeout"), Some(&"1000".to_string()));
        url.set_param("timeout", "3000");
        assert_eq!(url.get_param("timeout"), Some(&"3000".to_string()));
    }

    #[test]
    fn test_to_full_string_empty_params() {
        let mut url = URL::new("tri", "/myservice");
        url.ip = "127.0.0.1".to_string();
        url.port = "20880".to_string();
        url.set_param("version", "1.0.0");
        let full = url.to_full_string();
        assert_eq!(full, "tri://127.0.0.1:20880//myservice/1.0.0?version=1.0.0");
    }

    #[test]
    fn test_url_partial_eq() {
        let url1 = URL::new("tri", "/test");
        let url2 = URL::new("tri", "/test");
        assert_eq!(url1, url2);

        let mut url3 = URL::new("dubbo", "/test");
        url3.set_param("key", "val");
        assert_ne!(url1, url3);
    }
}
