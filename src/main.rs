use actix_web::rt::spawn;
use actix_web::{get, App, HttpResponse, HttpServer, Responder};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tokio::time::sleep;

#[derive(Debug, Deserialize)]
struct URadDataContainer {
    data: URadData,
}

#[derive(Debug, Deserialize, Serialize)]
struct URadData {
    temperature: f64,
    humidity: f64,
    voc: i32,
    co2: i32,
    ch2o: i32,
    o3: f64,
    pm1: f64,
    pm25: f64,
    pm10: f64,
    noise: f64,
}

#[derive(Debug, Serialize)]
struct DataEntry {
    timestamp: u128,
    #[serde(flatten)]
    data: URadData,
}

// TODO: Run as a Windows service
// https://github.com/mullvad/windows-service-rs
//
// Service installation:
// https://doc.sitecore.com/xp/en/developers/latest/sitecore-experience-manager/run-an-application-as-a-windows-service.html
// https://learn.microsoft.com/en-us/dotnet/framework/windows-services/how-to-install-and-uninstall-services
// https://stackoverflow.com/questions/8164859/install-a-windows-service-using-a-windows-command-prompt

#[actix_web::main]
async fn main() -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;

    let history = Vec::<DataEntry>::new();
    let history = actix_web::web::Data::new(RwLock::new(history));

    spawn({
        let history = history.clone();
        async move {
            loop {
                if let Ok(data) = fetch_data(&client).await {
                    let mut w = history.write().await;
                    w.push(data);
                }

                sleep(Duration::from_secs(1)).await;
            }
        }
    });

    HttpServer::new(move || App::new().service(get_data).app_data(history.clone()))
        .bind("127.0.0.1:8753")?
        .run()
        .await?;

    Ok(())
}

async fn fetch_data(client: &reqwest::Client) -> anyhow::Result<DataEntry> {
    let data = client.get("http://192.168.2.106/j").send().await?;

    let d = data.json::<URadDataContainer>().await?.data;
    Ok(DataEntry {
        timestamp: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)?
            .as_millis(),
        data: d,
    })
}

#[get("/")]
async fn get_data(history: actix_web::web::Data<RwLock<Vec<DataEntry>>>) -> impl Responder {
    let history = history.read().await;
    HttpResponse::Ok().body(serde_json::to_string(&*history).unwrap())
}
