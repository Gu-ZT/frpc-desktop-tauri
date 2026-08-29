//! Proxy service (ported from electron/service/ProxyService.ts).

use crate::core::business_error::BusinessError;
use crate::db::proxy_repository::ProxyRepository;
use crate::model::frp::FrpcProxy;
use crate::model::github::LocalPort;
use crate::service::frpc_process_service::FrpcProcessService;
use crate::service::system_service::SystemService;

#[derive(Clone)]
pub struct ProxyService {
    proxy_repo: ProxyRepository,
    frpc_process_service: FrpcProcessService,
    system_service: SystemService,
}

impl ProxyService {
    pub fn new(
        proxy_repo: ProxyRepository,
        frpc_process_service: FrpcProcessService,
        system_service: SystemService,
    ) -> Self {
        Self {
            proxy_repo,
            frpc_process_service,
            system_service,
        }
    }

    pub async fn insert_proxy(&self, mut proxy: FrpcProxy) -> Result<FrpcProxy, BusinessError> {
        let proxy2 = self
            .proxy_repo
            .insert(&mut proxy)
            .map_err(|e| BusinessError::internal(format!("insert proxy failed: {e}")))?;
        let _ = self.frpc_process_service.reload_frpc_process().await;
        Ok(proxy2)
    }

    pub async fn update_proxy(&self, mut proxy: FrpcProxy) -> Result<FrpcProxy, BusinessError> {
        let id = proxy.id.clone();
        let proxy2 = self
            .proxy_repo
            .update_by_id(&id, &mut proxy)
            .map_err(|e| BusinessError::internal(format!("update proxy failed: {e}")))?;
        let _ = self.frpc_process_service.reload_frpc_process().await;
        Ok(proxy2)
    }

    pub async fn update_proxy_status(&self, id: &str, status: i64) -> Result<(), BusinessError> {
        self.proxy_repo
            .update_proxy_status(id, status)
            .map_err(|e| BusinessError::internal(format!("update proxy status failed: {e}")))?;
        let _ = self.frpc_process_service.reload_frpc_process().await;
        Ok(())
    }

    pub async fn delete_proxy(&self, proxy_id: &str) -> Result<(), BusinessError> {
        self.proxy_repo
            .delete_by_id(proxy_id)
            .map_err(|e| BusinessError::internal(format!("delete proxy failed: {e}")))?;
        let _ = self.frpc_process_service.reload_frpc_process().await;
        Ok(())
    }

    pub async fn get_all_proxies(&self) -> Result<Vec<FrpcProxy>, BusinessError> {
        self.proxy_repo
            .find_all()
            .map_err(|e| BusinessError::internal(format!("load proxies failed: {e}")))
    }

    pub async fn get_local_ports(&self) -> Result<Vec<LocalPort>, BusinessError> {
        self.system_service
            .get_local_ports()
            .map_err(BusinessError::internal)
    }
}
