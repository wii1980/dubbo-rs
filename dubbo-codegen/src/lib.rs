pub use serde;

use std::collections::HashMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Public configuration types
// ---------------------------------------------------------------------------

/// Client code generation mode.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ClientMode {
    /// Generate channel-based client wrapping tonic generated client.
    Channel,
    /// Generate invoker-based client using `Box<dyn Invoker>`.
    Invoker,
    /// Generate both channel and invoker clients.
    #[default]
    Both,
}

#[derive(Clone, Debug)]
pub struct GeneratorConfig {
    pub proto_paths: Vec<PathBuf>,
    pub output_dir: Option<PathBuf>,
    pub enable_client: bool,
    pub enable_server: bool,
    pub client_mode: ClientMode,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            proto_paths: vec![PathBuf::from("proto")],
            output_dir: Some(PathBuf::from("src/gen")),
            enable_client: true,
            enable_server: true,
            client_mode: ClientMode::default(),
        }
    }
}

#[derive(Debug)]
pub struct GeneratorConfigBuilder {
    proto_paths: Vec<PathBuf>,
    output_dir: Option<PathBuf>,
    enable_client: bool,
    enable_server: bool,
    client_mode: ClientMode,
}

impl Default for GeneratorConfigBuilder {
    fn default() -> Self {
        Self {
            proto_paths: Vec::new(),
            output_dir: None,
            enable_client: true,
            enable_server: true,
            client_mode: ClientMode::default(),
        }
    }
}

impl GeneratorConfigBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn proto_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.proto_paths.push(path.into());
        self
    }

    #[must_use]
    pub fn output_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.output_dir = Some(dir.into());
        self
    }

    #[must_use]
    pub fn enable_client(mut self, enable: bool) -> Self {
        self.enable_client = enable;
        self
    }

    #[must_use]
    pub fn enable_server(mut self, enable: bool) -> Self {
        self.enable_server = enable;
        self
    }

    #[must_use]
    pub fn client_mode(mut self, mode: ClientMode) -> Self {
        self.client_mode = mode;
        self
    }

    /// # Errors
    ///
    /// Returns an error if `proto_paths` is empty.
    pub fn build(self) -> anyhow::Result<GeneratorConfig> {
        if self.proto_paths.is_empty() {
            anyhow::bail!("proto_paths must not be empty");
        }
        Ok(GeneratorConfig {
            proto_paths: self.proto_paths,
            output_dir: self.output_dir,
            enable_client: self.enable_client,
            enable_server: self.enable_server,
            client_mode: self.client_mode,
        })
    }
}

#[derive(Debug, Default)]
pub struct GeneratedCode {
    pub files: HashMap<String, String>,
}

impl GeneratedCode {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: String, content: String) {
        self.files.insert(name, content);
    }

    /// # Errors
    ///
    /// Returns an error if directory creation fails or if a file cannot be written.
    pub fn write_to_dir(&self, dir: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(dir)?;
        for (name, content) in &self.files {
            std::fs::write(dir.join(name), content)?;
        }
        Ok(())
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Parsed proto definitions
// ---------------------------------------------------------------------------

struct ProtoService {
    name: String,
    methods: Vec<ProtoMethod>,
}

struct ProtoMethod {
    name: String,
    input_type: String,
    output_type: String,
    client_streaming: bool,
    server_streaming: bool,
}

struct ProtoInfo {
    package: String,
    services: Vec<ProtoService>,
}

// ---------------------------------------------------------------------------
// String helpers
// ---------------------------------------------------------------------------

#[cfg(test)]
fn to_pascal_case(s: &str) -> String {
    s.split(['_', '-'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => {
                    let upper: String = c.to_uppercase().collect();
                    upper + chars.as_str()
                }
            }
        })
        .collect()
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + s.len() / 2);
    let chars: Vec<char> = s.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if c.is_uppercase() {
            let prev_lower = i > 0 && chars[i - 1].is_lowercase();
            let next_lower = i + 1 < chars.len() && chars[i + 1].is_lowercase();
            if i > 0 && (prev_lower || next_lower) {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(*c);
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Proto text parser
// ---------------------------------------------------------------------------

fn strip_comments(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '/' {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
        } else if i + 1 < chars.len() && chars[i] == '/' && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

fn parse_proto_content(content: &str) -> ProtoInfo {
    let cleaned = strip_comments(content);

    let package = regex_capture_package(&cleaned).unwrap_or_default();

    let services = parse_services(&cleaned);

    ProtoInfo { package, services }
}

fn regex_capture_package(input: &str) -> Option<String> {
    let pat = "package ";
    let start = input.find(pat)?;
    let rest = &input[start + pat.len()..];
    let end = rest.find(';')?;
    Some(rest[..end].trim().to_string())
}

fn parse_services(cleaned: &str) -> Vec<ProtoService> {
    let mut services = Vec::new();

    for svc_match in find_all_occurrences(cleaned, "service ") {
        if let Some(svc) = parse_one_service(cleaned, svc_match) {
            services.push(svc);
        }
    }

    services
}

fn find_all_occurrences(haystack: &str, needle: &str) -> Vec<usize> {
    haystack.match_indices(needle).map(|(i, _)| i).collect()
}

fn parse_one_service(cleaned: &str, start: usize) -> Option<ProtoService> {
    let remaining = &cleaned[start..];

    let first_brace = remaining.find('{')?;
    let header = &remaining[..first_brace];

    let name = header.trim().strip_prefix("service ")?.trim().to_string();

    let body_start = start + first_brace + 1;
    let brace_end = find_matching_brace(cleaned, body_start)?;
    let body = &cleaned[body_start..brace_end];

    let methods = parse_rpc_methods(body);

    Some(ProtoService { name, methods })
}

fn find_matching_brace(s: &str, start: usize) -> Option<usize> {
    let mut depth = 1i32;
    let chars: Vec<char> = s.chars().collect();
    let mut pos = start;
    while pos < chars.len() && depth > 0 {
        match chars.get(pos)? {
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
        pos += 1;
    }
    if depth == 0 {
        Some(pos - 1)
    } else {
        None
    }
}

fn parse_rpc_methods(body: &str) -> Vec<ProtoMethod> {
    let mut methods = Vec::new();

    for rpc_match in find_all_occurrences(body, "rpc ") {
        if let Some(method) = parse_one_rpc(body, rpc_match) {
            methods.push(method);
        }
    }

    methods
}

fn parse_one_rpc(body: &str, start: usize) -> Option<ProtoMethod> {
    let remaining = &body[start..];

    let semi_or_brace = remaining.find([';', '{']).unwrap_or(remaining.len());
    let rpc_text = &remaining[..semi_or_brace];

    // Format: rpc MethodName (stream? InputType) returns (stream? OutputType)
    let trimmed = rpc_text.trim().strip_prefix("rpc ")?;
    let paren_start = trimmed.find('(')?;
    let name = trimmed[..paren_start].trim().to_string();

    let rest = &trimmed[paren_start..];

    // Parse input: (stream? InputType)
    let inner = extract_between_parens(rest)?;
    let (client_streaming, input_type) = if inner.trim().starts_with("stream ") {
        (
            true,
            inner.trim().strip_prefix("stream ")?.trim().to_string(),
        )
    } else {
        (false, inner.trim().to_string())
    };

    // Find "returns" keyword
    let returns_pos = rest.find("returns")?;
    let returns_part = &rest[returns_pos + "returns".len()..];

    let output_inner = extract_between_parens(returns_part)?;
    let (server_streaming, output_type) = if output_inner.trim().starts_with("stream ") {
        (
            true,
            output_inner
                .trim()
                .strip_prefix("stream ")?
                .trim()
                .to_string(),
        )
    } else {
        (false, output_inner.trim().to_string())
    };

    Some(ProtoMethod {
        name,
        input_type,
        output_type,
        client_streaming,
        server_streaming,
    })
}

fn extract_between_parens(s: &str) -> Option<&str> {
    let start = s.find('(')?;
    let end = s[start + 1..].find(')')?;
    Some(&s[start + 1..start + 1 + end])
}

// ---------------------------------------------------------------------------
// Code generation
// ---------------------------------------------------------------------------

pub struct CodeGenerator {
    config: GeneratorConfig,
}

impl CodeGenerator {
    #[must_use]
    pub fn new(config: GeneratorConfig) -> Self {
        Self { config }
    }

    #[must_use]
    pub fn builder() -> GeneratorConfigBuilder {
        GeneratorConfigBuilder::new()
    }

    /// # Errors
    ///
    /// Returns an error if no proto files are found, if protobuf compilation fails,
    /// or if configured paths are invalid.
    pub fn generate(&self) -> anyhow::Result<GeneratedCode> {
        let mut generated = GeneratedCode::new();

        let (proto_files, include_dirs) = self.collect_proto_files()?;

        if proto_files.is_empty() {
            anyhow::bail!("no .proto files found in configured paths");
        }

        let proto_infos = Self::parse_proto_files(&proto_files)?;

        self.compile_with_tonic(&proto_files, &include_dirs)?;

        for proto_info in &proto_infos {
            if proto_info.services.is_empty() {
                continue;
            }
            let module_name = proto_info.package.replace('.', "_");
            let file_name = format!("{module_name}_dubbo.rs");
            let code = self.render_dubbo_integration(proto_info)?;
            generated.insert(file_name, code);
        }

        let build_rs = Self::render_build_rs_template(&proto_files, &include_dirs);
        generated.insert("build.rs.template".to_string(), build_rs);

        Ok(generated)
    }

    fn collect_proto_files(&self) -> anyhow::Result<(Vec<PathBuf>, Vec<PathBuf>)> {
        let mut proto_files: Vec<PathBuf> = Vec::new();
        let mut includes: Vec<PathBuf> = Vec::new();

        for path in &self.config.proto_paths {
            if path.is_dir() {
                includes.push(path.clone());
                for entry in std::fs::read_dir(path)? {
                    let entry = entry?;
                    let p = entry.path();
                    if p.extension().is_some_and(|e| e == "proto") {
                        proto_files.push(p);
                    }
                }
            } else if path.is_file() && path.extension().is_some_and(|e| e == "proto") {
                proto_files.push(path.clone());
                if let Some(parent) = path.parent() {
                    includes.push(parent.to_path_buf());
                }
            }
        }

        proto_files.sort();
        includes.sort();
        includes.dedup();

        Ok((proto_files, includes))
    }

    fn parse_proto_files(proto_files: &[PathBuf]) -> anyhow::Result<Vec<ProtoInfo>> {
        let mut infos = Vec::new();
        for pf in proto_files {
            let content = std::fs::read_to_string(pf)?;
            let info = parse_proto_content(&content);
            infos.push(info);
        }
        Ok(infos)
    }

    fn compile_with_tonic(
        &self,
        proto_files: &[PathBuf],
        include_dirs: &[PathBuf],
    ) -> anyhow::Result<()> {
        let proto_strs: Vec<&Path> = proto_files.iter().map(PathBuf::as_path).collect();

        let include_strs: Vec<&Path> = include_dirs.iter().map(PathBuf::as_path).collect();

        let mut builder = tonic_prost_build::configure();
        if let Some(ref out) = self.config.output_dir {
            builder = builder.out_dir(out);
        }

        builder.compile_protos(&proto_strs, &include_strs)?;

        Ok(())
    }

    fn render_dubbo_integration(&self, info: &ProtoInfo) -> anyhow::Result<String> {
        let mut code = String::new();
        let package = &info.package;
        let include_name = package;

        writeln!(code, "// Dubbo integration for `{package}` package.")?;
        writeln!(code, "// Generated by dubbo-rs-codegen. DO NOT EDIT.")?;
        writeln!(code)?;

        writeln!(code, "/// Proto types and tonic stubs.")?;
        writeln!(code, "#[allow(clippy::all, clippy::pedantic)]")?;
        writeln!(code, "pub mod proto {{")?;
        writeln!(code, "    tonic::include_proto!(\"{include_name}\");")?;
        writeln!(code, "}}")?;
        writeln!(code)?;

        if self.config.enable_server {
            Self::render_service_registration(&mut code, info)?;
        }

        if self.config.enable_client {
            match &self.config.client_mode {
                ClientMode::Channel | ClientMode::Both => {
                    Self::render_channel_client(&mut code, info)?;
                }
                ClientMode::Invoker => {}
            }
            match &self.config.client_mode {
                ClientMode::Invoker | ClientMode::Both => {
                    Self::render_invoker_client(&mut code, info)?;
                }
                ClientMode::Channel => {}
            }
        }

        Ok(code)
    }

    fn render_service_registration(code: &mut String, info: &ProtoInfo) -> std::fmt::Result {
        writeln!(code, "// === Service Registration ===")?;
        writeln!(code, "#[allow(clippy::all, clippy::pedantic)]")?;
        writeln!(code)?;

        for svc in &info.services {
            let svc_snake = to_snake_case(&svc.name);
            let svc_server_mod = format!("{svc_snake}_server");
            let svc_server_struct = format!("{}Server", svc.name);

            writeln!(
                code,
                "/// Register a `{}` service with a Dubbo server.",
                svc.name
            )?;
            writeln!(code, "#[allow(clippy::all, clippy::pedantic)]")?;
            writeln!(code, "pub fn register_{svc_snake}_service(")?;
            writeln!(code, "    server: dubbo_rs::server::Server,")?;
            writeln!(
                code,
                "    svc: impl proto::{svc_server_mod}::{name} + 'static,",
                name = svc.name
            )?;
            writeln!(code, ") -> dubbo_rs::server::Server {{")?;
            writeln!(code, "    server.register_service(|mut builder| {{")?;
            writeln!(
                code,
                "        builder.add_service(proto::{svc_server_mod}::{svc_server_struct}::new(svc))"
            )?;
            writeln!(code, "    }})")?;
            writeln!(code, "}}")?;
            writeln!(code)?;
        }

        Ok(())
    }

    fn render_channel_client(code: &mut String, info: &ProtoInfo) -> std::fmt::Result {
        writeln!(code, "// === Channel Client ===")?;
        writeln!(code, "#[allow(clippy::all, clippy::pedantic)]")?;
        writeln!(code)?;

        for svc in &info.services {
            let svc_snake = to_snake_case(&svc.name);
            let client_struct = format!("{}ChannelClient", svc.name);
            let tonic_client_mod = format!("{svc_snake}_client");
            let tonic_client_struct = format!("{}Client", svc.name);

            writeln!(
                code,
                "/// Channel-based Dubbo client for the `{}` service.",
                svc.name
            )?;
            writeln!(code, "#[allow(clippy::all, clippy::pedantic)]")?;
            writeln!(code, "pub struct {client_struct} {{")?;
            writeln!(
                code,
                "    inner: proto::{tonic_client_mod}::{tonic_client_struct}<tonic::transport::Channel>,"
            )?;
            writeln!(code, "}}")?;
            writeln!(code)?;

            writeln!(code, "#[allow(clippy::all, clippy::pedantic)]")?;
            writeln!(code, "impl {client_struct} {{")?;
            writeln!(
                code,
                "    /// Connect to the service at the given endpoint."
            )?;
            writeln!(
                code,
                "    pub async fn connect<D>(dst: D) -> Result<Self, tonic::transport::Error>"
            )?;
            writeln!(code, "    where")?;
            writeln!(code, "        D: TryInto<tonic::transport::Endpoint>,")?;
            writeln!(
                code,
                "        <D as TryInto<tonic::transport::Endpoint>>::Error:"
            )?;
            writeln!(
                code,
                "            Into<Box<dyn std::error::Error + Send + Sync>>,"
            )?;
            writeln!(code, "    {{")?;
            writeln!(
                code,
                "        let inner = proto::{tonic_client_mod}::{tonic_client_struct}::connect(dst).await?;"
            )?;
            writeln!(code, "        Ok(Self {{ inner }})")?;
            writeln!(code, "    }}")?;
            writeln!(code)?;

            writeln!(code, "    /// Create from an existing tonic channel.")?;
            writeln!(
                code,
                "    pub fn from_channel(channel: tonic::transport::Channel) -> Self {{"
            )?;
            writeln!(code, "        Self {{")?;
            writeln!(
                code,
                "            inner: proto::{tonic_client_mod}::{tonic_client_struct}::new(channel),"
            )?;
            writeln!(code, "        }}")?;
            writeln!(code, "    }}")?;
            writeln!(code)?;

            writeln!(code, "    /// Create from a Dubbo `Client`'s channel.")?;
            writeln!(
                code,
                "    pub fn from_dubbo_client(client: &dubbo_rs::client::Client) -> Option<Self> {{"
            )?;
            writeln!(
                code,
                "        client.channel().cloned().map(Self::from_channel)"
            )?;
            writeln!(code, "    }}")?;
            writeln!(code, "}}")?;
            writeln!(code)?;

            // RPC methods
            writeln!(code, "#[allow(clippy::all, clippy::pedantic)]")?;
            writeln!(code, "impl {client_struct} {{")?;
            for method in &svc.methods {
                Self::render_channel_method(code, info, svc, method)?;
            }
            writeln!(code, "}}")?;
            writeln!(code)?;
        }

        Ok(())
    }

    fn render_channel_method(
        code: &mut String,
        _info: &ProtoInfo,
        _svc: &ProtoService,
        method: &ProtoMethod,
    ) -> std::fmt::Result {
        let method_snake = to_snake_case(&method.name);

        if method.server_streaming {
            writeln!(code, "    pub async fn {method_snake}(")?;
            writeln!(code, "        &mut self,")?;
            writeln!(
                code,
                "        request: impl tonic::IntoRequest<proto::{}>,",
                method.input_type
            )?;
            writeln!(
                code,
                "    ) -> Result<tonic::Response<tonic::codec::Streaming<proto::{}>>, tonic::Status> {{",
                method.output_type
            )?;
            writeln!(code, "        self.inner.{method_snake}(request).await")?;
            writeln!(code, "    }}")?;
        } else if method.client_streaming && !method.server_streaming {
            writeln!(code, "    pub async fn {method_snake}(")?;
            writeln!(code, "        &mut self,")?;
            writeln!(
                code,
                "        request: impl tonic::IntoStreamingRequest<Message = proto::{}>,",
                method.input_type
            )?;
            writeln!(
                code,
                "    ) -> Result<tonic::Response<proto::{}>, tonic::Status> {{",
                method.output_type
            )?;
            writeln!(code, "        self.inner.{method_snake}(request).await")?;
            writeln!(code, "    }}")?;
        } else if method.client_streaming && method.server_streaming {
            // Bidi streaming
            writeln!(code, "    pub async fn {method_snake}(")?;
            writeln!(code, "        &mut self,")?;
            writeln!(
                code,
                "        request: impl tonic::IntoStreamingRequest<Message = proto::{}>,",
                method.input_type
            )?;
            writeln!(
                code,
                "    ) -> Result<tonic::Response<tonic::codec::Streaming<proto::{}>>, tonic::Status> {{",
                method.output_type
            )?;
            writeln!(code, "        self.inner.{method_snake}(request).await")?;
            writeln!(code, "    }}")?;
        } else {
            // Unary
            writeln!(code, "    pub async fn {method_snake}(")?;
            writeln!(code, "        &mut self,")?;
            writeln!(
                code,
                "        request: impl tonic::IntoRequest<proto::{}>,",
                method.input_type
            )?;
            writeln!(
                code,
                "    ) -> Result<tonic::Response<proto::{}>, tonic::Status> {{",
                method.output_type
            )?;
            writeln!(code, "        self.inner.{method_snake}(request).await")?;
            writeln!(code, "    }}")?;
        }

        Ok(())
    }

    fn render_invoker_client(code: &mut String, info: &ProtoInfo) -> std::fmt::Result {
        writeln!(code, "// === Invoker Client ===")?;
        writeln!(code, "#[allow(clippy::all, clippy::pedantic)]")?;
        writeln!(code)?;

        let package = &info.package;

        for svc in &info.services {
            let invoker_struct = format!("{}InvokerClient", svc.name);

            writeln!(
                code,
                "/// Invoker-based Dubbo client for the `{}` service.",
                svc.name
            )?;
            writeln!(code, "#[allow(clippy::all, clippy::pedantic)]")?;
            writeln!(code, "pub struct {invoker_struct} {{")?;
            writeln!(code, "    invoker: Box<dyn dubbo_rs::protocol::Invoker>,")?;
            writeln!(code, "}}")?;
            writeln!(code)?;

            writeln!(code, "#[allow(clippy::all, clippy::pedantic)]")?;
            writeln!(code, "impl {invoker_struct} {{")?;
            writeln!(code, "    /// Create a new client with the given invoker.")?;
            writeln!(
                code,
                "    pub fn new(invoker: Box<dyn dubbo_rs::protocol::Invoker>) -> Self {{"
            )?;
            writeln!(code, "        Self {{ invoker }}")?;
            writeln!(code, "    }}")?;
            writeln!(code)?;

            writeln!(code, "    /// Get a reference to the underlying invoker.")?;
            writeln!(
                code,
                "    pub fn invoker(&self) -> &dyn dubbo_rs::protocol::Invoker {{"
            )?;
            writeln!(code, "        self.invoker.as_ref()")?;
            writeln!(code, "    }}")?;
            writeln!(code, "}}")?;
            writeln!(code)?;

            // RPC methods
            writeln!(code, "#[allow(clippy::all, clippy::pedantic)]")?;
            writeln!(code, "impl {invoker_struct} {{")?;
            for method in &svc.methods {
                Self::render_invoker_method(code, package, svc, method)?;
            }
            writeln!(code, "}}")?;
            writeln!(code)?;
        }

        Ok(())
    }

    fn render_invoker_method(
        code: &mut String,
        package: &str,
        svc: &ProtoService,
        method: &ProtoMethod,
    ) -> std::fmt::Result {
        let method_snake = to_snake_case(&method.name);
        let method_path = format!(
            "/{package}.{svc_name}/{method_name}",
            svc_name = svc.name,
            method_name = method.name
        );

        if method.client_streaming || method.server_streaming {
            let stream_type = if method.client_streaming && method.server_streaming {
                "Bidirectional streaming"
            } else if method.server_streaming {
                "Server streaming"
            } else {
                "Client streaming"
            };
            let client_name = format!("{}ChannelClient", svc.name);
            writeln!(
                code,
                "    // Note: {stream_type} is not supported via Invoker."
            )?;
            writeln!(
                code,
                "    // Use {client_name} instead for streaming methods."
            )?;
            writeln!(code)?;
            return Ok(());
        }

        // Unary method
        writeln!(code, "    pub async fn {method_snake}(")?;
        writeln!(code, "        &self,")?;
        writeln!(code, "        request: proto::{},", method.input_type)?;
        writeln!(
            code,
            "    ) -> anyhow::Result<proto::{}> {{",
            method.output_type
        )?;
        writeln!(
            code,
            "        let mut ctx = dubbo_rs::protocol::InvocationContext::new("
        )?;
        writeln!(code, "            \"{method_path}\",")?;
        writeln!(code, "            self.invoker.get_url().clone(),")?;
        writeln!(code, "        );")?;
        writeln!(
            code,
            "        ctx.arguments = vec![prost::Message::encode_to_vec(&request)];"
        )?;
        writeln!(
            code,
            "        let result = self.invoker.invoke(&mut ctx).await?;"
        )?;
        writeln!(code, "        let value = result.value")?;
        writeln!(
            code,
            "            .ok_or_else(|| anyhow::anyhow!(\"empty response from {method_name}\"))?;",
            method_name = method.name
        )?;
        writeln!(code, "        Ok(prost::Message::decode(&value[..])?)")?;
        writeln!(code, "    }}")?;

        Ok(())
    }

    fn render_build_rs_template(proto_files: &[PathBuf], include_dirs: &[PathBuf]) -> String {
        let mut code = String::new();

        let proto_strs: Vec<String> = proto_files
            .iter()
            .filter_map(|p| p.to_str().map(String::from))
            .collect();
        let include_strs: Vec<String> = include_dirs
            .iter()
            .filter_map(|p| p.to_str().map(String::from))
            .collect();

        let _ = writeln!(code, "fn main() -> anyhow::Result<()> {{");
        let _ = writeln!(code, "    // Compile proto files with tonic-prost-build");
        let _ = writeln!(code, "    tonic_prost_build::compile_protos(");
        let _ = writeln!(code, "        &[");

        for ps in &proto_strs {
            let _ = writeln!(code, "            \"{ps}\",");
        }

        let _ = writeln!(code, "        ],");
        let _ = writeln!(code, "        &[");

        for inc in &include_strs {
            let _ = writeln!(code, "            \"{inc}\",");
        }

        let _ = writeln!(code, "        ],");
        let _ = writeln!(code, "    )?;");

        let _ = writeln!(code);
        let _ = writeln!(code, "    // Optional: generate Dubbo integration wrappers");
        let _ = writeln!(
            code,
            "    // let config = dubbo_rs_codegen::GeneratorConfigBuilder::new()"
        );
        for ps in &proto_strs {
            let _ = writeln!(code, "    //     .proto_path(\"{ps}\")");
        }
        let _ = writeln!(code, "    //     .build()?;");
        let _ = writeln!(
            code,
            "    // let generator = dubbo_rs_codegen::CodeGenerator::new(config);"
        );
        let _ = writeln!(code, "    // let generated = generator.generate()?;");
        let _ = writeln!(
            code,
            "    // generated.write_to_dir(std::path::Path::new(\"src/gen\"))?;"
        );

        let _ = writeln!(code);
        let _ = writeln!(code, "    Ok(())");
        let _ = writeln!(code, "}}");

        code
    }

    // -----------------------------------------------------------------------
    // Exposed for testing — parse a single proto string
    // -----------------------------------------------------------------------

    #[cfg(test)]
    fn parse_proto_string(content: &str) -> ProtoInfo {
        parse_proto_content(content)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // === Config / Builder tests ===

    #[test]
    fn test_builder_defaults() {
        let config = GeneratorConfig::default();
        assert_eq!(config.proto_paths.len(), 1);
        assert!(config.enable_client);
        assert!(config.enable_server);
        assert!(config.output_dir.is_some());
        assert_eq!(config.client_mode, ClientMode::Both);
    }

    #[test]
    fn test_builder_with_params() {
        let config = GeneratorConfigBuilder::new()
            .proto_path("api")
            .proto_path("shared")
            .output_dir("src/generated")
            .enable_client(false)
            .build()
            .unwrap();

        assert_eq!(config.proto_paths.len(), 2);
        assert!(!config.enable_client);
        assert!(config.enable_server);
    }

    #[test]
    fn test_builder_empty_proto_paths_fails() {
        let result = GeneratorConfigBuilder::new().build();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("proto_paths"));
    }

    #[test]
    fn test_builder_client_mode() {
        let config = GeneratorConfigBuilder::new()
            .proto_path("proto")
            .client_mode(ClientMode::Channel)
            .build()
            .unwrap();
        assert_eq!(config.client_mode, ClientMode::Channel);
    }

    #[test]
    fn test_client_mode_default() {
        assert_eq!(ClientMode::default(), ClientMode::Both);
    }

    // === GeneratedCode tests ===

    #[test]
    fn test_generated_code_empty() {
        let generated = GeneratedCode::new();
        assert!(generated.is_empty());
    }

    #[test]
    fn test_generated_code_insert() {
        let mut generated = GeneratedCode::new();
        generated.insert("svc.rs".into(), "// generated".into());
        assert!(!generated.is_empty());
        assert_eq!(generated.files.len(), 1);
    }

    #[test]
    fn test_code_generator_creation() {
        let config = GeneratorConfig::default();
        let cg = CodeGenerator::new(config);
        assert!(cg.config.enable_client);
    }

    #[test]
    fn test_generated_code_write_to_dir() {
        let mut generated = GeneratedCode::new();
        generated.insert("test.rs".into(), "fn test() {}".into());

        let dir = std::env::temp_dir().join(format!("dubbo-rs-codegen-test-{}", std::process::id()));
        generated.write_to_dir(&dir).unwrap();
        assert!(dir.join("test.rs").exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    // === Pascal case / snake case tests ===

    #[test]
    fn test_to_pascal_case_conversion() {
        assert_eq!(to_pascal_case("greeter"), "Greeter");
        assert_eq!(to_pascal_case("user_service"), "UserService");
        assert_eq!(to_pascal_case("helloworld"), "Helloworld");
        assert_eq!(to_pascal_case("my_complex_service"), "MyComplexService");
        assert_eq!(to_pascal_case("a"), "A");
        assert_eq!(to_pascal_case(""), "");
    }

    #[test]
    fn test_to_snake_case_conversion() {
        assert_eq!(to_snake_case("SayHello"), "say_hello");
        assert_eq!(to_snake_case("Greeter"), "greeter");
        assert_eq!(to_snake_case("TelephoneExchange"), "telephone_exchange");
        assert_eq!(to_snake_case("BidiStreamEcho"), "bidi_stream_echo");
        assert_eq!(to_snake_case("Echo"), "echo");
        assert_eq!(to_snake_case("RPC"), "rpc");
        assert_eq!(to_snake_case("GetUserByID"), "get_user_by_id");
    }

    // === Proto parser tests ===

    #[test]
    fn test_parse_unary_proto() {
        let proto = r#"
            syntax = "proto3";
            package greeter;
            service Greeter {
              rpc SayHello (HelloRequest) returns (HelloReply);
            }
        "#;
        let info = CodeGenerator::parse_proto_string(proto);
        assert_eq!(info.package, "greeter");
        assert_eq!(info.services.len(), 1);
        assert_eq!(info.services[0].name, "Greeter");
        assert_eq!(info.services[0].methods.len(), 1);
        assert_eq!(info.services[0].methods[0].name, "SayHello");
        assert_eq!(info.services[0].methods[0].input_type, "HelloRequest");
        assert_eq!(info.services[0].methods[0].output_type, "HelloReply");
        assert!(!info.services[0].methods[0].client_streaming);
        assert!(!info.services[0].methods[0].server_streaming);
    }

    #[test]
    fn test_parse_server_streaming_proto() {
        let proto = r#"
            syntax = "proto3";
            package exchange;
            service TelephoneExchange {
              rpc Dial(DialRequest) returns (stream DialProgress);
            }
        "#;
        let info = CodeGenerator::parse_proto_string(proto);
        assert_eq!(info.services[0].methods[0].name, "Dial");
        assert!(info.services[0].methods[0].server_streaming);
        assert!(!info.services[0].methods[0].client_streaming);
    }

    #[test]
    fn test_parse_all_rpc_types() {
        let proto = r#"
            syntax = "proto3";
            package triple.test;
            service TripleService {
              rpc Echo(EchoRequest) returns (EchoResponse);
              rpc ServerStreamEcho(EchoRequest) returns (stream EchoResponse);
              rpc ClientStreamEcho(stream EchoRequest) returns (EchoResponse);
              rpc BidiStreamEcho(stream EchoRequest) returns (stream EchoResponse);
            }
        "#;
        let info = CodeGenerator::parse_proto_string(proto);
        assert_eq!(info.package, "triple.test");
        let methods = &info.services[0].methods;
        assert_eq!(methods.len(), 4);

        assert_eq!(methods[0].name, "Echo");
        assert!(!methods[0].client_streaming);
        assert!(!methods[0].server_streaming);

        assert_eq!(methods[1].name, "ServerStreamEcho");
        assert!(!methods[1].client_streaming);
        assert!(methods[1].server_streaming);

        assert_eq!(methods[2].name, "ClientStreamEcho");
        assert!(methods[2].client_streaming);
        assert!(!methods[2].server_streaming);

        assert_eq!(methods[3].name, "BidiStreamEcho");
        assert!(methods[3].client_streaming);
        assert!(methods[3].server_streaming);
    }

    #[test]
    fn test_parse_with_comments() {
        let proto = r#"
            syntax = "proto3";
            // This is a comment
            package test;
            /* Multi-line
               comment */
            service Greeter {
              // Sends a greeting
              rpc SayHello (HelloRequest) returns (HelloReply);
            }
        "#;
        let info = CodeGenerator::parse_proto_string(proto);
        assert_eq!(info.package, "test");
        assert_eq!(info.services[0].methods.len(), 1);
    }

    #[test]
    fn test_parse_no_services() {
        let proto = r#"
            syntax = "proto3";
            package messages;
            message Empty {}
        "#;
        let info = CodeGenerator::parse_proto_string(proto);
        assert_eq!(info.package, "messages");
        assert!(info.services.is_empty());
    }

    #[test]
    fn test_parse_multiple_services() {
        let proto = r#"
            syntax = "proto3";
            package multi;
            service ServiceA {
              rpc MethodA(Req) returns (Res);
            }
            service ServiceB {
              rpc MethodB(Req) returns (Res);
            }
        "#;
        let info = CodeGenerator::parse_proto_string(proto);
        assert_eq!(info.services.len(), 2);
        assert_eq!(info.services[0].name, "ServiceA");
        assert_eq!(info.services[1].name, "ServiceB");
    }

    #[test]
    fn test_parse_nested_package() {
        let proto = r#"
            syntax = "proto3";
            package triple.test;
            service TripleService {
              rpc Echo(Req) returns (Res);
            }
        "#;
        let info = CodeGenerator::parse_proto_string(proto);
        assert_eq!(info.package, "triple.test");
    }

    // === Service registration renderer tests ===

    #[test]
    fn test_render_service_registration_unary() {
        let info = ProtoInfo {
            package: "greeter".to_string(),
            services: vec![ProtoService {
                name: "Greeter".to_string(),
                methods: vec![ProtoMethod {
                    name: "SayHello".to_string(),
                    input_type: "HelloRequest".to_string(),
                    output_type: "HelloReply".to_string(),
                    client_streaming: false,
                    server_streaming: false,
                }],
            }],
        };

        let config = GeneratorConfig {
            proto_paths: vec![PathBuf::from("proto")],
            output_dir: None,
            enable_client: false,
            enable_server: true,
            client_mode: ClientMode::Both,
        };
        let generator = CodeGenerator::new(config);
        let code = generator.render_dubbo_integration(&info).unwrap();

        assert!(code.contains("pub mod proto"));
        assert!(code.contains("tonic::include_proto!(\"greeter\")"));
        assert!(code.contains("pub fn register_greeter_service"));
        assert!(code.contains("svc: impl proto::greeter_server::Greeter + 'static"));
        assert!(
            code.contains("builder.add_service(proto::greeter_server::GreeterServer::new(svc))")
        );
    }

    #[test]
    fn test_render_service_registration_streaming() {
        let info = ProtoInfo {
            package: "exchange".to_string(),
            services: vec![ProtoService {
                name: "TelephoneExchange".to_string(),
                methods: vec![ProtoMethod {
                    name: "Dial".to_string(),
                    input_type: "DialRequest".to_string(),
                    output_type: "DialProgress".to_string(),
                    client_streaming: false,
                    server_streaming: true,
                }],
            }],
        };

        let config = GeneratorConfig {
            proto_paths: vec![PathBuf::from("proto")],
            output_dir: None,
            enable_client: false,
            enable_server: true,
            client_mode: ClientMode::Both,
        };
        let generator = CodeGenerator::new(config);
        let code = generator.render_dubbo_integration(&info).unwrap();

        assert!(code.contains("register_telephone_exchange_service"));
        assert!(code.contains("impl proto::telephone_exchange_server::TelephoneExchange + 'static"));
    }

    // === Channel client renderer tests ===

    #[test]
    fn test_render_channel_client_unary() {
        let info = ProtoInfo {
            package: "greeter".to_string(),
            services: vec![ProtoService {
                name: "Greeter".to_string(),
                methods: vec![ProtoMethod {
                    name: "SayHello".to_string(),
                    input_type: "HelloRequest".to_string(),
                    output_type: "HelloReply".to_string(),
                    client_streaming: false,
                    server_streaming: false,
                }],
            }],
        };

        let config = GeneratorConfig {
            proto_paths: vec![PathBuf::from("proto")],
            output_dir: None,
            enable_client: true,
            enable_server: false,
            client_mode: ClientMode::Channel,
        };
        let generator = CodeGenerator::new(config);
        let code = generator.render_dubbo_integration(&info).unwrap();

        assert!(code.contains("pub struct GreeterChannelClient"));
        assert!(
            code.contains("inner: proto::greeter_client::GreeterClient<tonic::transport::Channel>")
        );
        assert!(code.contains("pub async fn connect"));
        assert!(code.contains("pub fn from_channel"));
        assert!(code.contains("pub fn from_dubbo_client"));
        assert!(code.contains("pub async fn say_hello"));
        assert!(code.contains("impl tonic::IntoRequest<proto::HelloRequest>"));
        assert!(code.contains("Result<tonic::Response<proto::HelloReply>, tonic::Status>"));
    }

    #[test]
    fn test_render_channel_client_streaming() {
        let info = ProtoInfo {
            package: "exchange".to_string(),
            services: vec![ProtoService {
                name: "TelephoneExchange".to_string(),
                methods: vec![ProtoMethod {
                    name: "Dial".to_string(),
                    input_type: "DialRequest".to_string(),
                    output_type: "DialProgress".to_string(),
                    client_streaming: false,
                    server_streaming: true,
                }],
            }],
        };

        let config = GeneratorConfig {
            proto_paths: vec![PathBuf::from("proto")],
            output_dir: None,
            enable_client: true,
            enable_server: false,
            client_mode: ClientMode::Channel,
        };
        let generator = CodeGenerator::new(config);
        let code = generator.render_dubbo_integration(&info).unwrap();

        assert!(code.contains("TelephoneExchangeChannelClient"));
        assert!(code.contains("pub async fn dial"));
        assert!(code.contains("tonic::codec::Streaming<proto::DialProgress>"));
    }

    // === Invoker client renderer tests ===

    #[test]
    fn test_render_invoker_client_unary() {
        let info = ProtoInfo {
            package: "greeter".to_string(),
            services: vec![ProtoService {
                name: "Greeter".to_string(),
                methods: vec![ProtoMethod {
                    name: "SayHello".to_string(),
                    input_type: "HelloRequest".to_string(),
                    output_type: "HelloReply".to_string(),
                    client_streaming: false,
                    server_streaming: false,
                }],
            }],
        };

        let config = GeneratorConfig {
            proto_paths: vec![PathBuf::from("proto")],
            output_dir: None,
            enable_client: true,
            enable_server: false,
            client_mode: ClientMode::Invoker,
        };
        let generator = CodeGenerator::new(config);
        let code = generator.render_dubbo_integration(&info).unwrap();

        assert!(code.contains("pub struct GreeterInvokerClient"));
        assert!(code.contains("invoker: Box<dyn dubbo_rs::protocol::Invoker>"));
        assert!(code.contains("pub fn new(invoker: Box<dyn dubbo_rs::protocol::Invoker>) -> Self"));
        assert!(code.contains("pub async fn say_hello"));
        assert!(code.contains("request: proto::HelloRequest"));
        assert!(code.contains("prost::Message::encode_to_vec(&request)"));
        assert!(code.contains("prost::Message::decode(&value[..])"));
        assert!(code.contains("/greeter.Greeter/SayHello"));
    }

    #[test]
    fn test_render_invoker_client_streaming_not_supported() {
        let info = ProtoInfo {
            package: "exchange".to_string(),
            services: vec![ProtoService {
                name: "TelephoneExchange".to_string(),
                methods: vec![ProtoMethod {
                    name: "Dial".to_string(),
                    input_type: "DialRequest".to_string(),
                    output_type: "DialProgress".to_string(),
                    client_streaming: false,
                    server_streaming: true,
                }],
            }],
        };

        let config = GeneratorConfig {
            proto_paths: vec![PathBuf::from("proto")],
            output_dir: None,
            enable_client: true,
            enable_server: false,
            client_mode: ClientMode::Invoker,
        };
        let generator = CodeGenerator::new(config);
        let code = generator.render_dubbo_integration(&info).unwrap();

        assert!(code.contains("not supported via Invoker"));
        assert!(code.contains("TelephoneExchangeChannelClient"));
    }

    // === Full RPC types integration test ===

    #[test]
    fn test_render_all_rpc_types() {
        let info = ProtoInfo {
            package: "triple.test".to_string(),
            services: vec![ProtoService {
                name: "TripleService".to_string(),
                methods: vec![
                    ProtoMethod {
                        name: "Echo".to_string(),
                        input_type: "EchoRequest".to_string(),
                        output_type: "EchoResponse".to_string(),
                        client_streaming: false,
                        server_streaming: false,
                    },
                    ProtoMethod {
                        name: "ServerStreamEcho".to_string(),
                        input_type: "EchoRequest".to_string(),
                        output_type: "EchoResponse".to_string(),
                        client_streaming: false,
                        server_streaming: true,
                    },
                    ProtoMethod {
                        name: "ClientStreamEcho".to_string(),
                        input_type: "EchoRequest".to_string(),
                        output_type: "EchoResponse".to_string(),
                        client_streaming: true,
                        server_streaming: false,
                    },
                    ProtoMethod {
                        name: "BidiStreamEcho".to_string(),
                        input_type: "EchoRequest".to_string(),
                        output_type: "EchoResponse".to_string(),
                        client_streaming: true,
                        server_streaming: true,
                    },
                ],
            }],
        };

        let config = GeneratorConfig {
            proto_paths: vec![PathBuf::from("proto")],
            output_dir: None,
            enable_client: true,
            enable_server: true,
            client_mode: ClientMode::Both,
        };
        let generator = CodeGenerator::new(config);
        let code = generator.render_dubbo_integration(&info).unwrap();

        // Check module include
        assert!(code.contains("tonic::include_proto!(\"triple.test\")"));

        // Check registration
        assert!(code.contains("register_triple_service_service"));

        // Check channel client methods
        assert!(code.contains("pub async fn echo"));
        assert!(code.contains("pub async fn server_stream_echo"));
        assert!(code.contains("pub async fn client_stream_echo"));
        assert!(code.contains("pub async fn bidi_stream_echo"));

        // Check streaming return types
        assert!(code.contains("tonic::codec::Streaming<proto::EchoResponse>"));

        // Check invoker client: unary has method, streaming has comment
        assert!(code.contains("/triple.test.TripleService/Echo"));
        assert!(code.contains("not supported via Invoker"));
    }

    // === Nested package test ===

    #[test]
    fn test_nested_package_module_name() {
        let info = ProtoInfo {
            package: "triple.test".to_string(),
            services: vec![ProtoService {
                name: "TripleService".to_string(),
                methods: vec![ProtoMethod {
                    name: "Echo".to_string(),
                    input_type: "Req".to_string(),
                    output_type: "Res".to_string(),
                    client_streaming: false,
                    server_streaming: false,
                }],
            }],
        };

        let config = GeneratorConfig {
            proto_paths: vec![PathBuf::from("proto")],
            output_dir: None,
            enable_client: true,
            enable_server: true,
            client_mode: ClientMode::Both,
        };
        let generator = CodeGenerator::new(config);
        let code = generator.render_dubbo_integration(&info).unwrap();

        assert!(code.contains("tonic::include_proto!(\"triple.test\")"));
        assert!(code.contains("proto::triple_service_server::TripleService"));
        assert!(code.contains("proto::triple_service_client::TripleServiceClient"));
    }

    // === Build.rs template test ===

    #[test]
    fn test_render_build_rs_template() {
        let _generator = CodeGenerator::new(GeneratorConfig::default());
        let template = CodeGenerator::render_build_rs_template(
            &[PathBuf::from("proto/greeter.proto")],
            &[PathBuf::from("proto")],
        );
        assert!(template.contains("tonic_prost_build::compile_protos"));
        assert!(template.contains("proto/greeter.proto"));
        assert!(template.contains("dubbo_rs_codegen"));
    }

    // === No services produces no output ===

    #[test]
    fn test_render_no_services() {
        let info = ProtoInfo {
            package: "messages".to_string(),
            services: vec![],
        };

        let config = GeneratorConfig {
            proto_paths: vec![PathBuf::from("proto")],
            output_dir: None,
            enable_client: true,
            enable_server: true,
            client_mode: ClientMode::Both,
        };
        let generator = CodeGenerator::new(config);
        let code = generator.render_dubbo_integration(&info).unwrap();

        // Should still have the proto module but no service/client sections
        assert!(code.contains("pub mod proto"));
        assert!(!code.contains("register_"));
        assert!(!code.contains("ChannelClient"));
        assert!(!code.contains("InvokerClient"));
    }

    // === Edge case: method name casing ===

    #[test]
    fn test_method_name_snake_case_in_channel() {
        let info = ProtoInfo {
            package: "test".to_string(),
            services: vec![ProtoService {
                name: "TestService".to_string(),
                methods: vec![ProtoMethod {
                    name: "GetUserByID".to_string(),
                    input_type: "Req".to_string(),
                    output_type: "Res".to_string(),
                    client_streaming: false,
                    server_streaming: false,
                }],
            }],
        };

        let config = GeneratorConfig {
            proto_paths: vec![PathBuf::from("proto")],
            output_dir: None,
            enable_client: true,
            enable_server: false,
            client_mode: ClientMode::Channel,
        };
        let generator = CodeGenerator::new(config);
        let code = generator.render_dubbo_integration(&info).unwrap();

        assert!(code.contains("pub async fn get_user_by_id"));
        // Invoker path keeps original casing
    }

    #[test]
    fn test_invoker_method_path_casing() {
        let info = ProtoInfo {
            package: "greeter".to_string(),
            services: vec![ProtoService {
                name: "Greeter".to_string(),
                methods: vec![ProtoMethod {
                    name: "SayHello".to_string(),
                    input_type: "HelloRequest".to_string(),
                    output_type: "HelloReply".to_string(),
                    client_streaming: false,
                    server_streaming: false,
                }],
            }],
        };

        let config = GeneratorConfig {
            proto_paths: vec![PathBuf::from("proto")],
            output_dir: None,
            enable_client: true,
            enable_server: false,
            client_mode: ClientMode::Invoker,
        };
        let generator = CodeGenerator::new(config);
        let code = generator.render_dubbo_integration(&info).unwrap();

        // Method path uses original proto casing
        assert!(code.contains("/greeter.Greeter/SayHello"));
    }
}
