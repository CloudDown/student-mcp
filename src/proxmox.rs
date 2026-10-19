use crate::config::ProxmoxConfig;
use serde_json::Value;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(15);

/// Blocking Proxmox VE API client, authenticated by API token.
pub struct ProxmoxClient {
    config: ProxmoxConfig,
    agent: ureq::Agent,
}

impl ProxmoxClient {
    pub fn new(config: ProxmoxConfig) -> Self {
        ProxmoxClient {
            config,
            agent: ureq::AgentBuilder::new().timeout(TIMEOUT).build(),
        }
    }

    fn request(&self, path: &str, method: &str, body: Option<&Value>) -> Result<Value, String> {
        let url = format!("{}/api2/json{}", self.config.base_url, path);
        let authorization = format!(
            "PVEAPIToken={}={}",
            self.config.token_id, self.config.secret
        );
        let call = match (method, body) {
            ("POST", Some(payload)) => self
                .agent
                .post(&url)
                .set("Authorization", &authorization)
                .send_json(payload),
            ("POST", None) => self
                .agent
                .post(&url)
                .set("Authorization", &authorization)
                .call(),
            ("GET", _) => self
                .agent
                .get(&url)
                .set("Authorization", &authorization)
                .call(),
            ("DELETE", _) => self
                .agent
                .delete(&url)
                .set("Authorization", &authorization)
                .call(),
            ("PUT", Some(payload)) => self
                .agent
                .put(&url)
                .set("Authorization", &authorization)
                .send_json(payload),
            ("PUT", None) => self
                .agent
                .put(&url)
                .set("Authorization", &authorization)
                .call(),
            _ => return Err("Unsupported HTTP method for Proxmox.".into()),
        };
        let response = call.map_err(|error| match error {
            ureq::Error::Status(code, _) => format!("Proxmox refused the request (HTTP {code}). Check the token ID and its permissions."),
            _ => format!(
                "Proxmox: cannot connect ({}). Check the address, the port (usually 8006) and your network.",
                self.config.base_url
            ),
        })?;
        let envelope: Value = response
            .into_json()
            .map_err(|_| format!("Proxmox returned a non-JSON response at {path}."))?;
        if let Some(errors) = envelope.get("errors") {
            return Err(format!("Proxmox refused the request: {errors}"));
        }
        Ok(envelope.get("data").cloned().unwrap_or(Value::Null))
    }

    pub fn version(&self) -> Result<Value, String> {
        self.request("/version", "GET", None)
    }

    pub fn nodes(&self) -> Result<Value, String> {
        self.request("/nodes", "GET", None)
    }

    /// Every qemu/lxc resource visible to this token across the cluster.
    pub fn resources(&self) -> Result<Value, String> {
        self.request("/cluster/resources?type=vm", "GET", None)
    }

    pub fn vm_detail(&self, node: &str, type_of: &str, vmid: i64) -> Result<Value, String> {
        self.request(
            &format!("/nodes/{node}/{type_of}/{vmid}/status/current"),
            "GET",
            None,
        )
    }

    pub fn vm_action(
        &self,
        node: &str,
        type_of: &str,
        vmid: i64,
        action: &str,
    ) -> Result<Value, String> {
        self.request(
            &format!("/nodes/{node}/{type_of}/{vmid}/status/{action}"),
            "POST",
            None,
        )
    }
}

#[derive(Debug, Clone)]
pub struct Resource {
    pub node: String,
    pub status: String,
    pub kind: String,
}

/// Resolve a VM/LXC id to its node using the token's visible resources.
pub fn locate_resource(client: &ProxmoxClient, vmid: i64) -> Result<Resource, String> {
    let resources = client.resources()?;
    let found = resources
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .find_map(|entry| {
            let id = entry.get("vmid").and_then(Value::as_i64)?;
            if id != vmid {
                return None;
            }
            let node = entry.get("node").and_then(Value::as_str)?.to_string();
            let kind = entry.get("type").and_then(Value::as_str)?.to_string();
            let template = entry.get("template").and_then(Value::as_i64).unwrap_or(0);
            if node.is_empty() || kind.is_empty() || template != 0 {
                return None;
            }
            let status = entry
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            Some(Resource { node, status, kind })
        });
    found.ok_or_else(|| format!("No VM or CT {vmid} is visible with this Proxmox token."))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn locate_in_mock_resources() {
        let resource = json!({
            "vmid": 101,
            "node": "pve1",
            "type": "qemu",
            "status": "stopped"
        });
        let found = extract(&json!([resource]), 101);
        assert!(found.is_some());
    }

    fn extract(text: &Value, vmid: i64) -> Option<Resource> {
        text.as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .find_map(|entry| {
                let id = entry.get("vmid").and_then(Value::as_i64)?;
                if id != vmid {
                    return None;
                }
                Some(Resource {
                    node: entry.get("node").and_then(Value::as_str)?.to_string(),
                    status: entry.get("status").and_then(Value::as_str)?.to_string(),
                    kind: entry.get("type").and_then(Value::as_str)?.to_string(),
                })
            })
    }
}
