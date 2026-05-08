use anyhow::Result;
use dubbo_rs::common::url::URL;
use dubbo_rs::dubbo2::body;
use dubbo_rs::dubbo2::codec::SerializationId;
use dubbo_rs::dubbo2::transport::{DubboClient, DubboServer};
use dubbo_rs::remoting::{ExchangeClient, ExchangeServer, Request};

async fn run_server(port: u16) -> Result<()> {
    let mut url = URL::new("dubbo", "/com.example.Greeter");
    url.ip = "127.0.0.1".into();
    url.port = port.to_string();

    println!("[server] binding to 127.0.0.1:{port}");

    let server = DubboServer::new(SerializationId::Hessian2);
    server.bind(&url).await?;

    println!("[server] listening, press Ctrl+C to stop");
    tokio::signal::ctrl_c().await?;
    println!("[server] shutting down");
    server.close().await;

    Ok(())
}

async fn run_client(port: u16) -> Result<()> {
    let mut url = URL::new("dubbo", "/com.example.Greeter");
    url.ip = "127.0.0.1".into();
    url.port = port.to_string();

    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    println!("[client] connecting to 127.0.0.1:{port}");
    let mut client = DubboClient::new(SerializationId::Hessian2);
    client.connect(&url).await?;

    let service_url = URL::new("dubbo", "/com.example.Greeter");

    let ctx = dubbo_rs::protocol::InvocationContext::new("sayHello", service_url)
        .with_parameter_types(vec!["Ljava/lang/String;".to_string()])
        .with_arguments(vec![b"dubbo-rs".to_vec()]);

    let body_data = body::encode_request_body(&ctx)?;

    for i in 0..3 {
        let req = Request {
            id: i + 1,
            is_twoway: true,
            is_event: false,
            data: body_data.clone(),
        };

        let resp = client.request(req).await?;
        println!(
            "[client] request id={} → response status={}, body_len={}",
            i + 1,
            resp.status,
            resp.data.len()
        );
    }

    client.close();
    println!("[client] done.");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(20880);

    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "both".to_string());

    match mode.as_str() {
        "server" => run_server(port).await?,
        "client" => run_client(port).await?,
        _ => {
            let server_port = port;
            let client_port = port;

            let server = tokio::spawn(run_server(server_port));
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

            run_client(client_port).await?;

            server.abort();
        }
    }

    Ok(())
}
