//! Canary Release / Routing Demo.
//!
//! Demonstrates ConditionRouter (rule-based filtering), TagRouter (traffic coloring),
//! and RouterChain (chained filtering) for canary releases and traffic routing.
//!
//! This is a pure-logic demo — no external infrastructure required.
//!
//! Usage:
//!   cargo run -p canary-release

use std::sync::Arc;

use async_trait::async_trait;
use dubbo_rs::cluster::{ConditionRouter, RouterChain, TagRouter};
use dubbo_rs::common::node::Node;
use dubbo_rs::common::url::URL;
use dubbo_rs::protocol::{InvocationContext, Invoker, RPCResult};

fn make_url_with_params(host: &str, params: &[(&str, &str)]) -> URL {
    let mut url = URL::new("dubbo", "/com.example.Greeter");
    url.ip = host.to_string();
    url.port = "20880".to_string();
    for (k, v) in params {
        url.set_param(*k, *v);
    }
    url
}

struct RouterTestInvoker {
    url: URL,
}

impl RouterTestInvoker {
    fn new(url: URL) -> Self {
        Self { url }
    }
}

impl Node for RouterTestInvoker {
    fn get_url(&self) -> &URL {
        &self.url
    }
    fn is_available(&self) -> bool {
        true
    }
    fn destroy(&self) {}
}

#[async_trait]
impl Invoker for RouterTestInvoker {
    async fn invoke(&self, _ctx: &mut InvocationContext) -> Result<RPCResult, anyhow::Error> {
        Ok(RPCResult::success(
            format!("ok from {}", self.url.get_address()).into_bytes(),
        ))
    }
}

fn make_invokers(params_list: &[&[(&str, &str)]]) -> Vec<Arc<dyn Invoker>> {
    params_list
        .iter()
        .enumerate()
        .map(|(i, params)| {
            let host = format!("192.168.1.{}", i + 1);
            Arc::new(RouterTestInvoker::new(make_url_with_params(&host, params)))
                as Arc<dyn Invoker>
        })
        .collect()
}

fn describe_invokers(invokers: &[Arc<dyn Invoker>]) -> Vec<String> {
    invokers
        .iter()
        .map(|inv| {
            let url = inv.get_url();
            let env = url.get_param("env").map_or("none", |v| v.as_str());
            let tag = url.get_param("tag").map_or("none", |v| v.as_str());
            format!("{} (env={env}, tag={tag})", url.get_address())
        })
        .collect()
}

fn print_selection(indices: &[usize], invokers: &[Arc<dyn Invoker>]) {
    let descs = describe_invokers(invokers);
    if indices.is_empty() {
        println!("  → NO invokers matched");
    } else {
        println!("  → Selected:");
        for &i in indices {
            println!("    [{i}] {}", descs[i]);
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== dubbo-rs Canary Release / Routing Demo (Phase 3) ===\n");

    // ── Setup: 6 invokers representing different deployment groups ──
    let invokers = make_invokers(&[
        &[("env", "stable"), ("version", "v1.0")],
        &[("env", "stable"), ("version", "v2.0")],
        &[("env", "gray"), ("version", "v2.0")],
        &[("env", "gray"), ("version", "v2.0")],
        &[("env", "prod"), ("version", "v1.0"), ("tag", "green")],
        &[("env", "prod"), ("version", "v2.0"), ("tag", "blue")],
    ]);

    println!("Available invokers:");
    for (i, desc) in describe_invokers(&invokers).iter().enumerate() {
        println!("  [{i}] {desc}");
    }

    // ── Demo 1: ConditionRouter — canary release routing ───────────
    println!("\n── Demo 1: ConditionRouter — route to gray instances ──");
    {
        let router = ConditionRouter::parse("=> env=gray").expect("invalid condition route rule");
        println!("  Rule: \"=> env=gray\" (always match, filter to env=gray)");
        let indices = router.filter_invokers(&invokers);
        print_selection(&indices, &invokers);
    }

    // ── Demo 2: ConditionRouter — region-based routing ─────────────
    println!("\n── Demo 2: ConditionRouter — region-triggered routing ──");
    {
        let router = ConditionRouter::parse("region=beijing => env=stable")
            .expect("invalid condition route rule");
        println!("  Rule: \"region=beijing => env=stable\"");

        let mut url = URL::new("dubbo", "/com.example.Greeter");
        url.ip = "10.0.0.1".into();
        let mut ctx = InvocationContext::new("sayHello", url);

        // Client from beijing
        ctx.attachments.insert("region".into(), "beijing".into());
        let matches = router.matches_invocation(&ctx);
        println!("  Client region=beijing → rule matches: {matches}");
        if matches {
            let indices = router.filter_invokers(&invokers);
            print_selection(&indices, &invokers);
        }

        // Client from shanghai
        ctx.attachments.insert("region".into(), "shanghai".into());
        let matches = router.matches_invocation(&ctx);
        println!("  Client region=shanghai → rule matches: {matches} (skipped)");
    }

    // ── Demo 3: ConditionRouter — multi-condition ──────────────────
    println!("\n── Demo 3: ConditionRouter — double condition ──");
    {
        let router = ConditionRouter::parse("region=beijing,source=pre => version=v2.0")
            .expect("invalid condition route rule");
        println!("  Rule: \"region=beijing,source=pre => version=v2.0\"");

        let mut url = URL::new("dubbo", "/com.example.Greeter");
        url.ip = "10.0.0.1".into();
        let mut ctx = InvocationContext::new("sayHello", url);

        // Both conditions met
        ctx.attachments.insert("region".into(), "beijing".into());
        ctx.attachments.insert("source".into(), "pre".into());
        println!(
            "  Client region=beijing,source=pre → matches: {}",
            router.matches_invocation(&ctx)
        );

        // Only one condition met
        ctx.attachments.insert("source".into(), "prod".into());
        println!(
            "  Client region=beijing,source=prod → matches: {}",
            router.matches_invocation(&ctx)
        );
    }

    // ── Demo 4: TagRouter — traffic coloring ───────────────────────
    println!("\n── Demo 4: TagRouter — traffic coloring ──");
    {
        let tag_router = TagRouter::default();
        println!("  Tag key: \"dubbo.tag\"");

        // Request v2
        let mut url = URL::new("dubbo", "/com.example.Greeter");
        url.ip = "10.0.0.1".into();
        let mut ctx = InvocationContext::new("sayHello", url);
        ctx.attachments.insert("dubbo.tag".into(), "blue".into());
        println!("  Client requests dubbo.tag=blue:");

        // Only invokers 4-5 have tag params, only invoker 5 has blue
        let indices = tag_router.route(&invokers, &ctx);
        print_selection(&indices, &invokers);

        // Request non-existent tag → fallback to untagged
        ctx.attachments
            .insert("dubbo.tag".into(), "nonexistent".into());
        println!("\n  Client requests dubbo.tag=nonexistent (fallback to untagged):");
        let indices = tag_router.route(&invokers, &ctx);
        print_selection(&indices, &invokers);
    }

    // ── Demo 5: RouterChain — condition + tag combined ─────────────
    println!("\n── Demo 5: RouterChain — condition THEN tag ──");
    {
        let chain = RouterChain::new()
            .with_condition_router(
                ConditionRouter::parse("=> env=prod").expect("invalid condition route rule"),
            )
            .with_tag_router(TagRouter::default());

        println!("  Chain: ConditionRouter(=> env=prod) → TagRouter(default)");

        let mut url = URL::new("dubbo", "/com.example.Greeter");
        url.ip = "10.0.0.1".into();
        let mut ctx = InvocationContext::new("sayHello", url);
        ctx.attachments.insert("dubbo.tag".into(), "blue".into());

        println!("  Client requests dubbo.tag=blue through prod env:");
        let indices = chain.route(&invokers, &ctx);
        print_selection(&indices, &invokers);

        // Different tag
        ctx.attachments.insert("dubbo.tag".into(), "green".into());
        println!("\n  Client requests dubbo.tag=green through prod env:");
        let indices = chain.route(&invokers, &ctx);
        print_selection(&indices, &invokers);
    }

    // ── Demo 6: Gray release example — new version rollout ─────────
    println!("\n── Demo 6: Canary Release Scenario — roll out v2.0 ──");
    {
        let gray_router =
            ConditionRouter::parse("=> env=gray").expect("invalid condition route rule");
        let stable_router =
            ConditionRouter::parse("=> env=stable").expect("invalid condition route rule");

        println!("  Gray  pool (env=gray):");
        print_selection(&gray_router.filter_invokers(&invokers), &invokers);
        println!("  Stable pool (env=stable):");
        print_selection(&stable_router.filter_invokers(&invokers), &invokers);
    }

    println!("\n=== All canary release demos complete ===");
    Ok(())
}
