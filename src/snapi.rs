use std::path::Path;

use crate::error::SdkError;

async fn parse_json_or_sse(resp: reqwest::Response) -> Result<serde_json::Value, SdkError> {
    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();

    let body = resp
        .text()
        .await
        .map_err(|e| SdkError::Serialization(e.to_string()))?;

    if content_type.contains("application/json") {
        return serde_json::from_str(&body).map_err(|e| SdkError::Serialization(e.to_string()));
    }

    // sn-api status endpoints can stream SSE: "data: {json}\\n\\n"
    if content_type.contains("text/event-stream") || body.contains("data:") {
        let mut last_json: Option<&str> = None;
        for line in body.lines() {
            if let Some(rest) = line.strip_prefix("data:") {
                let candidate = rest.trim();
                if !candidate.is_empty() {
                    last_json = Some(candidate);
                }
            }
        }
        if let Some(j) = last_json {
            return serde_json::from_str(j).map_err(|e| SdkError::Serialization(e.to_string()));
        }
    }

    serde_json::from_str(&body).map_err(|e| {
        SdkError::Serialization(format!("unsupported status body format: {e}; body={body}"))
    })
}

#[derive(Clone)]
pub struct SnApiClient {
    pub base: String,
    http: reqwest::Client,
}

impl SnApiClient {
    pub fn new(base: String) -> Self {
        Self {
            base,
            http: reqwest::Client::new(),
        }
    }

    pub async fn upload_status(&self, task_id: &str) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .http
            .get(format!(
                "{}/api/v1/actions/cascade/tasks/{}/status",
                self.base.trim_end_matches('/'),
                task_id
            ))
            .send()
            .await
            .map_err(|e| SdkError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(SdkError::Http(format!(
                "upload_status failed: {}",
                resp.status()
            )));
        }
        parse_json_or_sse(resp).await
    }

    pub async fn download_status(&self, task_id: &str) -> Result<serde_json::Value, SdkError> {
        let resp = self
            .http
            .get(format!(
                "{}/api/v1/downloads/cascade/{}/status",
                self.base.trim_end_matches('/'),
                task_id
            ))
            .send()
            .await
            .map_err(|e| SdkError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(SdkError::Http(format!(
                "download_status failed: {}",
                resp.status()
            )));
        }
        parse_json_or_sse(resp).await
    }

    pub async fn download_file(&self, task_id: &str) -> Result<Vec<u8>, SdkError> {
        let resp = self
            .http
            .get(format!(
                "{}/api/v1/downloads/cascade/{}/file",
                self.base.trim_end_matches('/'),
                task_id
            ))
            .send()
            .await
            .map_err(|e| SdkError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(SdkError::Http(format!(
                "download_file failed: {}",
                resp.status()
            )));
        }
        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|e| SdkError::Http(e.to_string()))
    }

    pub async fn start_cascade_bytes(
        &self,
        action_id: &str,
        signature: &str,
        file_name: &str,
        file_bytes: Vec<u8>,
    ) -> Result<String, SdkError> {
        let part = reqwest::multipart::Part::bytes(file_bytes).file_name(file_name.to_string());
        let form = reqwest::multipart::Form::new()
            .text("action_id", action_id.to_string())
            .text("signature", signature.to_string())
            .part("file", part);

        let resp = self
            .http
            .post(format!(
                "{}/api/v1/actions/cascade",
                self.base.trim_end_matches('/')
            ))
            .multipart(form)
            .send()
            .await
            .map_err(|e| SdkError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(SdkError::Http(format!(
                "start_cascade failed: {}",
                resp.status()
            )));
        }

        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| SdkError::Serialization(e.to_string()))?;
        let task_id = v
            .get("task_id")
            .and_then(|x| x.as_str())
            .or_else(|| v.get("taskId").and_then(|x| x.as_str()))
            .unwrap_or_default()
            .to_string();
        if task_id.is_empty() {
            return Err(SdkError::Serialization("missing task id".into()));
        }
        Ok(task_id)
    }

    pub async fn start_cascade(
        &self,
        action_id: &str,
        signature: &str,
        file_path: &Path,
    ) -> Result<String, SdkError> {
        let bytes = tokio::fs::read(file_path)
            .await
            .map_err(|e| SdkError::Http(e.to_string()))?;
        let file_name = file_path
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or("upload.bin")
            .to_string();

        self.start_cascade_bytes(action_id, signature, &file_name, bytes)
            .await
    }

    pub async fn request_download(
        &self,
        action_id: &str,
        signature: &str,
    ) -> Result<String, SdkError> {
        // Download can race with finalization/indexing right after upload completion.
        // Retry transient 5xx responses briefly before failing.
        let url = format!(
            "{}/api/v1/actions/cascade/{}/downloads",
            self.base.trim_end_matches('/'),
            action_id
        );
        let mut last_err = String::new();

        for attempt in 1..=10 {
            let resp = self
                .http
                .post(&url)
                .json(&serde_json::json!({"signature": signature}))
                .send()
                .await
                .map_err(|e| SdkError::Http(e.to_string()))?;

            let status = resp.status();
            if status.is_success() {
                let v: serde_json::Value = resp
                    .json()
                    .await
                    .map_err(|e| SdkError::Serialization(e.to_string()))?;
                let task_id = v
                    .get("task_id")
                    .and_then(|x| x.as_str())
                    .or_else(|| v.get("taskId").and_then(|x| x.as_str()))
                    .unwrap_or_default()
                    .to_string();
                if task_id.is_empty() {
                    return Err(SdkError::Serialization("missing task id".into()));
                }
                return Ok(task_id);
            }

            let body = resp.text().await.unwrap_or_default();
            last_err = format!("request_download failed: {} body={}", status, body);

            if status.is_server_error() && attempt < 10 {
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
            return Err(SdkError::Http(last_err));
        }

        Err(SdkError::Http(last_err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    #[tokio::test]
    async fn tdd_start_cascade_parses_task() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/actions/cascade"))
            .respond_with(
                ResponseTemplate::new(202).set_body_json(serde_json::json!({"task_id":"t1"})),
            )
            .mount(&server)
            .await;

        let dir = tempdir().unwrap();
        let fp = dir.path().join("a.bin");
        tokio::fs::write(&fp, b"abc").await.unwrap();
        let c = SnApiClient::new(server.uri());
        let task = c.start_cascade("a1", "sig", &fp).await.unwrap();
        assert_eq!(task, "t1");
    }

    #[tokio::test]
    async fn tdd_start_cascade_bytes_parses_task() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/actions/cascade"))
            .respond_with(
                ResponseTemplate::new(202).set_body_json(serde_json::json!({"task_id":"t2"})),
            )
            .mount(&server)
            .await;

        let c = SnApiClient::new(server.uri());
        let task = c
            .start_cascade_bytes("a2", "sig", "file.bin", b"hello".to_vec())
            .await
            .unwrap();
        assert_eq!(task, "t2");
    }

    #[tokio::test]
    async fn tdd_request_download_retries_transient_5xx() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/api/v1/actions/cascade/99/downloads"))
            .respond_with(ResponseTemplate::new(500).set_body_string("not ready"))
            .up_to_n_times(2)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/actions/cascade/99/downloads"))
            .respond_with(
                ResponseTemplate::new(202).set_body_json(serde_json::json!({"task_id":"d99"})),
            )
            .mount(&server)
            .await;

        let c = SnApiClient::new(server.uri());
        let task = c.request_download("99", "sig").await.unwrap();
        assert_eq!(task, "d99");
    }
}
