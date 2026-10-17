use crate::{error::AppError, synchronization::Push, vocabulary::digest};
use reqwest::blocking::{Client, Response};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::io::Read;

pub(crate) const CHUNK_BYTES: usize = 500_000;
pub(crate) use crate::sync_delta::MAX_PACKET_BYTES;
const DIRECT_BYTES: usize = 996_000;

#[cfg(test)]
thread_local! {
    static LIVE_TRANSFERS: std::cell::RefCell<Vec<Value>> = const { std::cell::RefCell::new(Vec::new()) };
}
#[cfg(test)]
fn record_transfer(direction: &str, value: &Value) {
    if value.get("entryId").is_some()
        || value.get("sessionId").is_some()
        || value.get("bundle").is_some()
    {
        LIVE_TRANSFERS.with(|v| {
            v.borrow_mut().push(json!({
                "direction": direction,
                "delta": value.get("delta").is_some() || value.get("deltaRef").is_some(),
                "bytes": serde_json::to_vec(value).unwrap().len()
            }));
        });
    }
}
#[cfg(test)]
pub(crate) fn take_live_transfers() -> Vec<Value> {
    LIVE_TRANSFERS.with(|v| std::mem::take(&mut *v.borrow_mut()))
}

fn failure(code: &str, message: impl Into<String>) -> AppError {
    AppError::new(code, message)
}

pub(crate) fn read_json(response: Response) -> Result<(u16, Value), AppError> {
    let status = response.status().as_u16();
    let mut bytes = vec![];
    response
        .take(1_010_001)
        .read_to_end(&mut bytes)
        .map_err(|_| failure("network_unavailable", "站点响应中断，待同步内容保留。"))?;
    if bytes.len() > 1_010_000 {
        return Err(failure("invalid_data", "同步响应过大，已保留当前资料。"));
    }
    if status == 401 || status == 403 || (300..400).contains(&status) {
        return Err(failure(
            "auth_required",
            "站点账号连接需要重新批准，本地内容与同步进度保留。",
        ));
    }
    if status == 413 {
        return Err(failure(
            "capacity_exceeded",
            "词条资料超过本次同步容量，全部本地资料保留。",
        ));
    }
    if !matches!(status, 200..=299 | 409 | 412) {
        return Err(failure(
            "network_error",
            format!("同步未完成（HTTP {status}），本地资料与待处理内容保留。"),
        ));
    }
    Ok((
        status,
        serde_json::from_slice(&bytes)
            .map_err(|_| failure("invalid_data", "站点同步响应不是有效资料。"))?,
    ))
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PacketReference {
    digest: String,
    size: usize,
    parts: usize,
}
impl PacketReference {
    fn validate(&self) -> Result<(), AppError> {
        if self.size == 0
            || self.size > MAX_PACKET_BYTES
            || self.parts != self.size.div_ceil(CHUNK_BYTES)
            || self.digest.len() != 64
            || !self
                .digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(failure("invalid_data", "同步分片清单无效，当前游标保留。"));
        }
        Ok(())
    }
}

pub(crate) struct Transport<'a> {
    client: &'a Client,
    site: &'a str,
    token: &'a str,
}
impl<'a> Transport<'a> {
    pub(crate) fn new(client: &'a Client, site: &'a str, token: &'a str) -> Self {
        Self {
            client,
            site,
            token,
        }
    }

    pub(crate) fn fetch(
        &self,
        path: &str,
        check: impl Fn() -> Result<(), AppError>,
    ) -> Result<Value, AppError> {
        self.fetch_since(path, None, check)
    }
    pub(crate) fn fetch_since(
        &self,
        path: &str,
        base: Option<(i64, &Value)>,
        check: impl Fn() -> Result<(), AppError>,
    ) -> Result<Value, AppError> {
        check()?;
        let mut request = self.client.get(format!("{}{path}", self.site)).bearer_auth(self.token);
        if let Some((revision,_)) = base {
            request = request.header("x-svl-delta", crate::sync_delta::PROTOCOL).header("x-svl-base-revision", revision.to_string());
        }
        let (_, value) = read_json(
            request.send()
                .map_err(|_| failure("network_unavailable", "拉取未完成，本地资料与游标保留。"))?,
        )?;
        #[cfg(test)]
        record_transfer("download", &value);
        let value = self.hydrate(value, &check)?;
        check()?;
        crate::sync_delta::restore_document(value, base)
    }

    fn hydrate(
        &self,
        mut value: Value,
        check: impl Fn() -> Result<(), AppError>,
    ) -> Result<Value, AppError> {
        check()?;
        let is_delta = value.get("deltaRef").is_some();
        let field = if is_delta { "deltaRef" } else { "packetRef" };
        let Some(reference) = value.get(field) else {
            return Ok(value);
        };
        let reference: PacketReference = serde_json::from_value(reference.clone())?;
        reference.validate()?;
        let mut bytes = Vec::with_capacity(reference.size);
        for index in 0..reference.parts {
            check()?;
            let response = self
                .client
                .get(format!(
                    "{}/api/svl/v1/packets/{}/{index}",
                    self.site, reference.digest
                ))
                .bearer_auth(self.token)
                .send()
                .map_err(|_| failure("network_unavailable", "资料分片下载中断，游标保留。"))?;
            if !response.status().is_success() {
                read_json(response)?;
                return Err(failure("resource_missing", "资料分片不可用，游标保留。"));
            }
            if response
                .headers()
                .get("x-svl-packet-sha256")
                .and_then(|h| h.to_str().ok())
                != Some(reference.digest.as_str())
                || response
                    .headers()
                    .get("content-type")
                    .and_then(|h| h.to_str().ok())
                    != Some("application/octet-stream")
            {
                return Err(failure("invalid_data", "资料分片标记不匹配，游标保留。"));
            }
            let expected = CHUNK_BYTES.min(reference.size - index * CHUNK_BYTES);
            let mut part = vec![];
            response
                .take(expected as u64 + 1)
                .read_to_end(&mut part)
                .map_err(|_| failure("network_unavailable", "资料分片下载中断，游标保留。"))?;
            if part.len() != expected {
                return Err(failure("invalid_data", "资料分片长度不匹配，游标保留。"));
            }
            bytes.extend_from_slice(&part);
        }
        check()?;
        if digest(&bytes) != reference.digest {
            return Err(failure("invalid_data", "完整资料摘要不匹配，游标保留。"));
        }
        let bundle: Value = serde_json::from_slice(&bytes)?;
        if !is_delta && ((value.get("sessionId").is_some() && bundle["session"]["id"] != value["sessionId"])
            || (value.get("sessionId").is_none() && bundle["entry"]["id"] != value["entryId"])
        ) {
            return Err(failure("invalid_data", "资料分片关联的词条不一致。"));
        }
        value
            .as_object_mut()
            .ok_or_else(|| failure("invalid_data", "同步响应结构无效。"))?
            .remove(field);
        value[if is_delta { "delta" } else { "bundle" }] = bundle;
        Ok(value)
    }

    pub(crate) fn push(
        &self,
        request: &Push,
        base: Option<&Value>,
        supports_chunks: bool,
        check: impl Fn() -> Result<(), AppError>,
    ) -> Result<(u16, Value), AppError> {
        self.push_changes(
            &format!(
                "/api/svl/v1/entries/{}",
                request.bundle.entry["id"].as_str().unwrap()
            ),
            &serde_json::to_value(request)?,
            base,
            "entryId",
            supports_chunks,
            check,
        )
    }
    pub(crate) fn push_changes(
        &self,
        path: &str,
        full: &Value,
        base: Option<&Value>,
        identity: &str,
        supports_chunks: bool,
        check: impl Fn() -> Result<(), AppError>,
    ) -> Result<(u16, Value), AppError> {
        // Keep the full logical request and its capacity/receipt semantics unchanged.
        if serde_json::to_vec(full)?.len() > MAX_PACKET_BYTES {
            return Err(failure("capacity_exceeded", "完整资料超过同步容量，全部内容保留。"));
        }
        let wire = crate::sync_delta::request(full, base, identity);
        let reply = self.push_value(path, &wire, supports_chunks, &check)?;
        if reply.0 == 412 && reply.1["error"] == "delta_base_missing" && wire.get("delta").is_some() {
            return self.push_value(path, full, supports_chunks, check);
        }
        if reply.0 == 412 { return Err(failure("invalid_data", "站点未确认同步基准，待发送资料保留。")); }
        Ok(reply)
    }
    pub(crate) fn push_value(
        &self,
        path: &str,
        request: &Value,
        supports_chunks: bool,
        check: impl Fn() -> Result<(), AppError>,
    ) -> Result<(u16, Value), AppError> {
        check()?;
        let bytes = serde_json::to_vec(request)?;
        #[cfg(test)]
        record_transfer("upload", request);
        if bytes.len() > MAX_PACKET_BYTES || bytes.len() > DIRECT_BYTES && !supports_chunks {
            return Err(failure(
                "capacity_exceeded",
                "完整资料超过本次同步能力，未删减任何例句、原声或历史。",
            ));
        }
        let body = if bytes.len() <= DIRECT_BYTES {
            serde_json::to_value(request)?
        } else {
            let reference = PacketReference {
                digest: digest(&bytes),
                size: bytes.len(),
                parts: bytes.len().div_ceil(CHUNK_BYTES),
            };
            for (index, part) in bytes.chunks(CHUNK_BYTES).enumerate() {
                check()?;
                let part_digest = digest(part);
                let url = format!(
                    "{}/api/svl/v1/packets/{}/{index}",
                    self.site, reference.digest
                );
                let head = self
                    .client
                    .head(&url)
                    .bearer_auth(self.token)
                    .send()
                    .map_err(|_| {
                        failure("network_unavailable", "无法检查资料分片，待同步内容保留。")
                    })?;
                if head.status().is_success() {
                    if head
                        .headers()
                        .get("x-content-sha256")
                        .and_then(|h| h.to_str().ok())
                        != Some(part_digest.as_str())
                        || head
                            .headers()
                            .get("content-length")
                            .and_then(|h| h.to_str().ok())
                            .and_then(|v| v.parse::<usize>().ok())
                            != Some(part.len())
                    {
                        return Err(failure(
                            "invalid_data",
                            "已上传资料分片不一致，待同步内容保留。",
                        ));
                    }
                } else if head.status().as_u16() == 404 {
                    check()?;
                    let (_, reply) = read_json(
                        self.client
                            .put(&url)
                            .bearer_auth(self.token)
                            .header("content-type", "application/octet-stream")
                            .header("x-content-sha256", &part_digest)
                            .body(part.to_vec())
                            .send()
                            .map_err(|_| {
                                failure("network_unavailable", "资料分片上传中断，同一内容可重试。")
                            })?,
                    )?;
                    if reply["packetDigest"] != reference.digest
                        || reply["index"].as_u64() != Some(index as u64)
                        || reply["digest"] != part_digest
                        || reply["size"].as_u64() != Some(part.len() as u64)
                    {
                        return Err(failure(
                            "invalid_data",
                            "资料分片回执无效，待同步内容保留。",
                        ));
                    }
                } else {
                    read_json(head)?;
                    return Err(failure("network_error", "站点资料分片状态无效。"));
                }
            }
            json!({"packetRef": reference})
        };
        check()?;
        let (status, value) = read_json(
            self.client
                .put(format!("{}{path}", self.site))
                .bearer_auth(self.token)
                .json(&body)
                .send()
                .map_err(|_| {
                    failure(
                        "network_unavailable",
                        "推送响应丢失或中断，已冻结的同一内容可安全重试。",
                    )
                })?,
        )?;
        Ok((status, self.hydrate(value, check)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{store::Store, sync_test_fixture};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    #[ignore = "Normal local cross-language integration; set SVL_DOWNLOAD_FIXTURE to the Site route output"]
    fn delta_download_normal_history_roundtrip() {
        use crate::synchronization::{Remote,RemoteChange,RemoteDocument,ChangePage,PushReply};
        use std::{io::{Read,Write},net::TcpListener,sync::atomic::AtomicBool};
        let fixture: Value = serde_json::from_slice(&std::fs::read(std::env::var("SVL_DOWNLOAD_FIXTURE").expect("Site-generated fixture path")).unwrap()).unwrap();
        struct Received { events: Vec<RemoteDocument>, audio: Vec<u8> }
        impl Remote for Received {
            fn changes(&mut self, after:i64)->Result<ChangePage,AppError>{Ok(ChangePage{changes:self.events.iter().enumerate().filter(|(i,_)|*i as i64+1>after).map(|(i,d)|RemoteChange{cursor:i as i64+1,entry_id:d.entry_id.clone(),revision:d.revision,change_id:format!("synthetic-{i}")}).collect(),backfill_pending:false})}
            fn change(&mut self, change:&RemoteChange)->Result<RemoteDocument,AppError>{Ok(self.events[change.cursor as usize-1].clone())}
            fn push(&mut self, _: &Push)->Result<PushReply,AppError>{panic!("Receiving history must not create a new collection or upload")}
            fn upload(&mut self,_:&str,_:&str,_:Vec<u8>)->Result<(),AppError>{panic!("Unchanged originals must not be uploaded")}
            fn download(&mut self,_:&str,_:&str)->Result<Vec<u8>,AppError>{Ok(self.audio.clone())}
        }
        let audio:Vec<u8>=serde_json::from_value(fixture["audio"]["bytes"].clone()).unwrap();
        let mut remote=Received{events:vec![],audio};
        for scenario in fixture["cases"].as_array().unwrap() {
            let dir=tempfile::tempdir().unwrap();
            let store=Store::open(dir.path()).unwrap();
            let assessment=scenario["kind"]=="assessment";
            let id=scenario["id"].as_str().unwrap();
            if assessment { store.assessment_sync_capability("download-fixture",Some("assessment-1")).unwrap(); }
            for (index,step) in scenario["steps"].as_array().unwrap().iter().enumerate() {
                let base=store.sync_download_base("download-fixture",id,assessment).unwrap();
                let listener=TcpListener::bind("127.0.0.1:0").unwrap();
                let site=format!("http://{}",listener.local_addr().unwrap());
                let body=serde_json::to_vec(&step["document"]).unwrap();
                let server=std::thread::spawn(move||{
                    let (mut socket,_)=listener.accept().unwrap();
                    socket.set_read_timeout(Some(std::time::Duration::from_secs(10))).unwrap();
                    let mut request=Vec::new();let mut part=[0u8;1024];
                    while !request.windows(4).any(|v|v==b"\r\n\r\n") {
                        let count=socket.read(&mut part).unwrap();assert!(count>0);request.extend_from_slice(&part[..count]);
                    }
                    let request=String::from_utf8(request).unwrap().to_lowercase();
                    if index>0 {assert!(request.contains("x-svl-delta: json-delta-1"));assert!(request.contains(&format!("x-svl-base-revision: {index}\r\n")));}
                    write!(socket,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
                    socket.write_all(&body).unwrap();
                });
                let client=Client::builder().no_proxy().timeout(std::time::Duration::from_secs(10)).build().unwrap();
                let document=Transport::new(&client,&site,"synthetic-token").fetch_since("/synthetic-change",base.as_ref().map(|(r,b)|(*r,b)),||Ok(())).unwrap();
                server.join().unwrap();
                assert_eq!(document["bundle"],step["expected"]);
                if assessment {
                    let doc:crate::assessment_sync::AssessmentDocument=serde_json::from_value(document).unwrap();
                    doc.bundle.validate().unwrap();
                    store.assessment_sync_receive("download-fixture",&doc).unwrap();
                    store.assessment_sync_cursor("download-fixture",index as i64+1).unwrap();
                } else {
                    remote.events.push(serde_json::from_value(document).unwrap());
                    let result=store.synchronize("download-fixture",&mut remote,&AtomicBool::new(false),|_,_|{}).unwrap();
                    assert_eq!(result.cursor,index as i64+1);
                }
                let (revision,bundle)=store.sync_download_base("download-fixture",id,assessment).unwrap().unwrap();
                assert_eq!(revision,index as i64+1);
                assert_eq!(bundle,step["expected"]);
            }
            if assessment {
                assert_eq!(store.assessment_sync_status("download-fixture").unwrap().cursor,3);
            } else {
                let c=store.connection().unwrap();
                let asset:String=c.query_row("SELECT id FROM media_assets WHERE digest=?",[fixture["audio"]["digest"].as_str().unwrap()],|r|r.get(0)).unwrap();
                drop(c);
                assert_eq!(std::fs::read(store.media_file(&asset).unwrap()).unwrap(),remote.audio);
                assert_eq!(store.list_entries("",0,10).unwrap().len(),1);
            }
        }
    }

    #[test]
    #[ignore = "Requires the local synthetic serve-sync-fixture.mjs backend"]
    fn http_chunk_retries_cancellation_and_corruption_preserve_complete_packet() {
        let site = std::env::var("SVL_PROTOCOL_FIXTURE_URL").expect("local synthetic fixture URL");
        assert!(site.starts_with("http://127.0.0.1:"));
        let token = "synthetic_svl_protocol_fixture_token_01234567890123456789";
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let transport = Transport::new(&client, &site, token);
        let directory = tempfile::tempdir().unwrap();
        let store = Store::open(directory.path()).unwrap();
        let entry = sync_test_fixture::large_entry(&store, None, 1800);
        let push = Push {
            change_id: uuid::Uuid::new_v4().to_string(),
            base_revision: 0,
            bundle: store.sync_export(&entry).unwrap(),
        };
        let bytes = serde_json::to_vec(&push).unwrap();
        assert!(bytes.len() > 2_000_000 && bytes.len() < MAX_PACKET_BYTES);
        assert_eq!(
            transport.push(&push, None, false, || Ok(())).unwrap_err().code,
            "capacity_exceeded"
        );
        let calls = AtomicUsize::new(0);
        let stopped = transport.push(&push, None, true, || {
            if calls.fetch_add(1, Ordering::Relaxed) >= 4 {
                Err(AppError::new("cancelled", "synthetic cancellation"))
            } else {
                Ok(())
            }
        });
        assert_eq!(stopped.unwrap_err().code, "cancelled");
        client
            .post(format!("{site}/__fixture/fault"))
            .bearer_auth(token)
            .json(&json!({"dropUploadIndex":1}))
            .send()
            .unwrap();
        assert_eq!(
            transport.push(&push, None, true, || Ok(())).unwrap_err().code,
            "network_error"
        );
        assert_eq!(transport.push(&push, None, true, || Ok(())).unwrap().0, 200);
        let state: Value = client
            .get(format!("{site}/__fixture/state"))
            .bearer_auth(token)
            .send()
            .unwrap()
            .json()
            .unwrap();
        for index in 0..2 {
            assert_eq!(
                state["uploads"][format!("/api/svl/v1/packets/{}/{index}", digest(&bytes))]
                    .as_u64(),
                Some(1)
            );
        }
        client
            .post(format!("{site}/__fixture/fault"))
            .bearer_auth(token)
            .json(&json!({"corruptDownload":true}))
            .send()
            .unwrap();
        assert_eq!(
            transport
                .fetch(&format!("/api/svl/v1/entries/{entry}"), || Ok(()))
                .unwrap_err()
                .code,
            "invalid_data"
        );
        let returned = transport
            .fetch(&format!("/api/svl/v1/entries/{entry}"), || Ok(()))
            .unwrap();
        assert_eq!(returned["revision"], 1);
        assert_eq!(
            returned["bundle"],
            serde_json::to_value(&push.bundle).unwrap()
        );
        assert_eq!(
            returned["bundle"]["tables"]["review_attempts"]
                .as_array()
                .unwrap()
                .len(),
            1800
        );
    }
}
