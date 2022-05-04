use actix_web::web::Data;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use awc::http::StatusCode;
use clap::Parser;
use env_logger::Env;
use futures_util::stream::TryStreamExt;
use log::{debug, info, warn};

/// Simple reserve proxy
#[derive(Clone, Parser)]
struct Config {
    /// Bind address
    #[clap(short, long, default_value = "127.0.0.1")]
    address: String,
    /// Listen on port
    #[clap(short, long, default_value = "4242")]
    port: u16,
    /// Proxy requests to
    #[clap(short, long, default_value = "http://localhost:8000")]
    to: String,
}

async fn proxy(
    req: HttpRequest,
    body: web::Payload,
    config: Data<Config>,
    http_client: Data<awc::Client>,
) -> impl Responder {
    // Stream request from the client to the proxied server
    let url = format!(
        "{to}{path}",
        to = config.to,
        path = req.uri().path_and_query().map(|p| p.as_str()).unwrap_or("")
    );
    debug!("=> {url}");
    match http_client
        .request_from(&url, req.head())
        .send_stream(body)
        .await
    {
        Ok(resp) => {
            // Stream response back to the client
            let status = resp.status();
            debug!("<= [{status}] {url}", status = status.as_u16());
            let mut resp_builder = HttpResponse::build(status);
            for header in resp.headers() {
                resp_builder.insert_header(header);
            }
            resp_builder.streaming(resp.into_stream())
        }
        Err(err) => {
            warn!("{url}: {err:?}");
            HttpResponse::build(StatusCode::BAD_GATEWAY).body("Bad Gateway")
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let config = Config::parse();
    let Config { address, port, to } = config.clone();
    info!("Listening on {address}:{port}");
    info!("Proxying requests to {to}");

    HttpServer::new(move || {
        let http_client = awc::Client::default();
        App::new()
            .app_data(Data::new(config.clone()))
            .app_data(Data::new(http_client))
            .service(web::resource("{path:.*}").to(proxy))
    })
    .bind((address, port))?
    .run()
    .await
}
