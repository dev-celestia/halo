//! R2 object-storage accessors.

use super::*;

impl BackendR2 for NativeBackend {
    // ── R2 Storage ─────────────────────────────────────────────────────────

    fn r2_credentials_status(
        &self,
    ) -> BridgeResult<Option<halo_backend::commands::r2::R2CredentialsStatus>> {
        self.tokio
            .block_on(commands::r2::credentials_status(&self.runtime))
            .map_err(BridgeError::message)
    }

    fn r2_save_credentials(
        &self,
        account_id: String,
        access_key_id: String,
        secret_access_key: Option<String>,
        custom_endpoint_url: Option<String>,
    ) -> BridgeResult<()> {
        self.tokio
            .block_on(commands::r2::save_credentials(
                &self.runtime,
                account_id,
                access_key_id,
                secret_access_key,
                custom_endpoint_url,
            ))
            .map_err(BridgeError::message)
    }

    fn r2_clear_credentials(&self) -> BridgeResult<()> {
        self.tokio
            .block_on(commands::r2::clear_credentials(&self.runtime))
            .map_err(BridgeError::message)
    }

    fn r2_list_buckets(&self) -> BridgeResult<Vec<String>> {
        self.tokio
            .block_on(commands::r2::list_buckets(&self.runtime))
            .map_err(BridgeError::message)
    }

    fn r2_list_objects(
        &self,
        bucket: String,
        prefix: String,
    ) -> BridgeResult<halo_backend::commands::r2::R2Listing> {
        self.tokio
            .block_on(commands::r2::list_objects(&self.runtime, bucket, prefix))
            .map_err(BridgeError::message)
    }

    fn r2_create_bucket(&self, name: String) -> BridgeResult<()> {
        self.tokio
            .block_on(commands::r2::create_bucket(&self.runtime, name))
            .map_err(BridgeError::message)
    }

    fn r2_delete_bucket(&self, name: String) -> BridgeResult<()> {
        self.tokio
            .block_on(commands::r2::delete_bucket(&self.runtime, name))
            .map_err(BridgeError::message)
    }

    fn r2_create_folder(&self, bucket: String, key: String) -> BridgeResult<()> {
        self.tokio
            .block_on(commands::r2::create_folder(&self.runtime, bucket, key))
            .map_err(BridgeError::message)
    }

    fn r2_delete_object(&self, bucket: String, key: String) -> BridgeResult<()> {
        self.tokio
            .block_on(commands::r2::delete_object(&self.runtime, bucket, key))
            .map_err(BridgeError::message)
    }

    fn r2_upload_file(
        &self,
        bucket: String,
        key: String,
        source_path: String,
        content_type: Option<String>,
    ) -> BridgeResult<()> {
        self.tokio
            .block_on(commands::r2::upload_file(
                &self.runtime,
                bucket,
                key,
                source_path,
                content_type,
            ))
            .map_err(BridgeError::message)
    }

    fn r2_download_object(&self, bucket: String, key: String) -> BridgeResult<String> {
        self.tokio
            .block_on(commands::r2::download_object(&self.runtime, bucket, key))
            .map_err(BridgeError::message)
    }

    fn r2_presign_url(
        &self,
        bucket: String,
        key: String,
        expires_seconds: u64,
    ) -> BridgeResult<String> {
        self.tokio
            .block_on(commands::r2::presign_url(
                &self.runtime,
                bucket,
                key,
                expires_seconds,
            ))
            .map_err(BridgeError::message)
    }
}
