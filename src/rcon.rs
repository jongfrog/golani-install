use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{timeout, Duration},
};

use crate::error::{Error, Result};

pub async fn command(address: &str, password: &str, command: &str) -> Result<String> {
    let mut stream = timeout(Duration::from_secs(5), TcpStream::connect(address))
        .await
        .map_err(|_| Error::ServerCreation("RCON 연결 시간이 초과되었습니다".into()))??;
    write_packet(&mut stream, 1, 3, password).await?;
    let (id, _) = read_packet(&mut stream).await?;
    if id != 1 {
        return Err(Error::ServerCreation("RCON 인증에 실패했습니다".into()));
    }
    write_packet(&mut stream, 2, 2, command).await?;
    let (id, body) = read_packet(&mut stream).await?;
    if id != 2 {
        return Err(Error::ServerCreation(
            "RCON 명령 응답이 올바르지 않습니다".into(),
        ));
    }
    Ok(body)
}

async fn write_packet(stream: &mut TcpStream, id: i32, kind: i32, body: &str) -> Result<()> {
    let length = 4 + 4 + body.len() + 2;
    stream.write_all(&(length as i32).to_le_bytes()).await?;
    stream.write_all(&id.to_le_bytes()).await?;
    stream.write_all(&kind.to_le_bytes()).await?;
    stream.write_all(body.as_bytes()).await?;
    stream.write_all(&[0, 0]).await?;
    Ok(())
}

async fn read_packet(stream: &mut TcpStream) -> Result<(i32, String)> {
    let mut length = [0; 4];
    stream.read_exact(&mut length).await?;
    let length = i32::from_le_bytes(length);
    if !(10..=4096).contains(&length) {
        return Err(Error::ServerCreation(
            "RCON 패킷 길이가 올바르지 않습니다".into(),
        ));
    }
    let mut data = vec![0; length as usize];
    stream.read_exact(&mut data).await?;
    let id = i32::from_le_bytes(data[0..4].try_into().unwrap());
    let body = String::from_utf8_lossy(&data[8..data.len() - 2]).into_owned();
    Ok((id, body))
}
