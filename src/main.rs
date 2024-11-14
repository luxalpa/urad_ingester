use actix_web::rt::spawn;
use actix_web::{get, App, HttpResponse, HttpServer, Responder};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::sync::mpsc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::ServiceControlHandlerResult;
use windows_service::{define_windows_service, service_control_handler, service_dispatcher};

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

// Service installation:
// https://doc.sitecore.com/xp/en/developers/latest/sitecore-experience-manager/run-an-application-as-a-windows-service.html
// https://learn.microsoft.com/en-us/dotnet/framework/windows-services/how-to-install-and-uninstall-services
// https://stackoverflow.com/questions/8164859/install-a-windows-service-using-a-windows-command-prompt

define_windows_service!(ffi_service_main, my_service_main);

fn my_service_main(arguments: Vec<OsString>) {
    if let Err(_e) = run_service(arguments) {
        // Handle errors in some way.
    }
}

const SERVICE_NAME: &str = "urad_ingester";

fn run_service(_arguments: Vec<OsString>) -> Result<(), windows_service::Error> {
    let (tx, rx) = mpsc::channel();

    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        match control_event {
            ServiceControl::Stop => {
                tx.send(()).unwrap();
                ServiceControlHandlerResult::NoError
            }
            // All services must accept Interrogate even if it's a no-op.
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    // Register system service event handler
    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

    let next_status = ServiceStatus {
        // Should match the one from system service registry
        service_type: ServiceType::OWN_PROCESS,
        // The new state
        current_state: ServiceState::Running,
        // Accept stop events when running
        controls_accepted: ServiceControlAccept::STOP,
        // Used to report an error when starting or stopping only, otherwise must be zero
        exit_code: ServiceExitCode::Win32(0),
        // Only used for pending states, otherwise must be zero
        checkpoint: 0,
        // Only used for pending states, otherwise must be zero
        wait_hint: Duration::default(),
        process_id: None,
    };

    // Tell the system that the service is running now
    status_handle.set_service_status(next_status)?;

    let _ = run_webserver(rx);

    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    })?;

    Ok(())
}

fn main() -> anyhow::Result<()> {
    // let (tx, rx) = mpsc::channel();
    //
    // std::thread::spawn(move || {
    //     std::thread::sleep(Duration::from_secs(3));
    //     tx.send(()).unwrap();
    // });
    //
    // let _ = run_webserver(rx);

    service_dispatcher::start(SERVICE_NAME, ffi_service_main)?;
    Ok(())
}

#[actix_web::main]
async fn run_webserver(rx: mpsc::Receiver<()>) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()?;

    let history = Vec::<DataEntry>::new();
    let history = actix_web::web::Data::new(RwLock::new(history));

    let server = HttpServer::new({
        let history = history.clone();
        move || App::new().service(get_data).app_data(history.clone())
    })
    .bind("127.0.0.1:8753")?
    .run();

    let handle = server.handle();

    spawn({
        let history = history.clone();
        async move {
            loop {
                if let Ok(data) = fetch_data(&client).await {
                    let mut w = history.write().await;
                    w.push(data);
                }

                match rx.recv_timeout(Duration::from_secs(1)) {
                    Ok(_) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                        break;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
            }

            handle.stop(false).await;
        }
    });

    server.await?;

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
