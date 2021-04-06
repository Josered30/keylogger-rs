
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use tokio::{fs::{File}, io::{AsyncBufReadExt, AsyncWriteExt, BufReader, BufWriter}, sync::Mutex};

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


pub async fn send_header_data(log_file: Arc<Mutex<LogFile>>) -> Result<bool, reqwest::Error> {
    use winapi::um::winnls::GetUserDefaultLocaleName;

    let os_info = {
        let info = os_info::get();
        format!("OS: type: {} - Version: {}", info.os_type(), info.version())
    };

    let hostname = format!(
        "Hostname: {}",
        hostname::get_hostname().unwrap_or("_NO_HOSTNAME_".to_string())
    );

    let locale = unsafe {
        const LEN: i32 = 85; //from https://docs.microsoft.com/de-de/windows/desktop/Intl/locale-name-constants
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

    let metadata: Vec<String> = vec![os_info, hostname, locale];
    let locked_file = log_file.lock().await;

    let info = Info {
        metadata,
        filename: locked_file.filename.clone(),
        logs: Vec::new(),
    };

    let target = "http://localhost:5000/api/logs".to_string();
    let client = reqwest::Client::new();
    let response = client
        .post(&target)
        .json(&info)
        .send()
        .await?
        .json::<Response>()
        .await?;
    return Ok(response.status);
}


pub async fn send_info_data(file: Arc<Mutex<LogFile>>) -> Result<bool, reqwest::Error> {
    let mut logs: Vec<String> = Vec::new();
    let lock_file = file.lock().await;
    
    let mut clone_file = lock_file.file.try_clone().await.expect("Async error");
    clone_file.seek(std::io::SeekFrom::Start(0)).await.expect("Async error");

    let reader = BufReader::new(clone_file);
    let mut lines = reader.lines();
    while let Some(element) = lines.next_line().await.expect("Async error") {
        logs.push(element);    
    }

    let info = Info {
        metadata: Vec::new(),
        filename: lock_file.filename.clone(),
        logs,
    };

    let target = "http://localhost:5000/api/logs".to_string();
    let client = reqwest::Client::new();
    let response = client
        .post(&target)
        .json(&info)
        .send()
        .await?
        .json::<Response>()
        .await?;

    clone_file = lock_file.file.try_clone().await.expect("Async error");
    clone_file.seek(std::io::SeekFrom::Start(0)).await.expect("Async error");
    clone_file.set_len(0).await.expect("Async error");

    return Ok(response.status);
}

pub async fn log(file: Arc<Mutex<LogFile>>, s: String) -> Result<(), tokio::io::Error> {
    #[cfg(debug_assertions)]
    {
        print!("{}", s);
    }

    let lock_file = file.lock().await;
    let mut writer = BufWriter::new(lock_file.file.try_clone().await.expect("Async error"));

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