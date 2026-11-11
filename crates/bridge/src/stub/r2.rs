//! Stub R2 object-storage accessors (always unavailable).

use super::*;

impl BackendR2 for StubBackend {
    fn r2_credentials_status(
        &self,
    ) -> BridgeResult<Option<halo_backend::commands::r2::R2CredentialsStatus>> {
        Ok(None)
    }

    fn r2_save_credentials(
        &self,
        _account_id: String,
        _access_key_id: String,
        _secret_access_key: Option<String>,
        _custom_endpoint_url: Option<String>,
    ) -> BridgeResult<()> {
        Err(self.r2_unavailable())
    }

    fn r2_clear_credentials(&self) -> BridgeResult<()> {
        Err(self.r2_unavailable())
    }

    fn r2_list_buckets(&self) -> BridgeResult<Vec<String>> {
        Err(self.r2_unavailable())
    }

    fn r2_list_objects(
        &self,
        _bucket: String,
        _prefix: String,
    ) -> BridgeResult<halo_backend::commands::r2::R2Listing> {
        Err(self.r2_unavailable())
    }

    fn r2_create_bucket(&self, _name: String) -> BridgeResult<()> {
        Err(self.r2_unavailable())
    }

    fn r2_delete_bucket(&self, _name: String) -> BridgeResult<()> {
        Err(self.r2_unavailable())
    }

    fn r2_create_folder(&self, _bucket: String, _key: String) -> BridgeResult<()> {
        Err(self.r2_unavailable())
    }

    fn r2_delete_object(&self, _bucket: String, _key: String) -> BridgeResult<()> {
        Err(self.r2_unavailable())
    }

    fn r2_upload_file(
        &self,
        _bucket: String,
        _key: String,
        _source_path: String,
        _content_type: Option<String>,
    ) -> BridgeResult<()> {
        Err(self.r2_unavailable())
    }

    fn r2_download_object(&self, _bucket: String, _key: String) -> BridgeResult<String> {
        Err(self.r2_unavailable())
    }

    fn r2_presign_url(
        &self,
        _bucket: String,
        _key: String,
        _expires_seconds: u64,
    ) -> BridgeResult<String> {
        Err(self.r2_unavailable())
    }
}
