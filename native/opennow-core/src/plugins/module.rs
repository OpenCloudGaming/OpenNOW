use super::Changes;
use super::process::ProcessModule;
use super::provider_process::ProviderProcess;
use super::transport::Transport;
use crate::requests::Cancellation;
use crate::sources::contract::{CatalogSource, SourceError};
use opennow_plugin_api::{CatalogPage, CatalogQuery, PluginDescriptor};
use opennow_plugin_package::InstalledManifest;
use std::path::Path;
use std::sync::Arc;

pub(super) enum Module {
    Catalog(Arc<ProcessModule>),
    Provider(Arc<ProviderProcess>),
}

impl Module {
    pub(super) fn new(
        manifest: &InstalledManifest,
        descriptor: PluginDescriptor,
        changes: Arc<Changes>,
    ) -> Self {
        match manifest {
            InstalledManifest::Catalog(_) => {
                Self::Catalog(Arc::new(ProcessModule::new(descriptor, changes)))
            }
            InstalledManifest::Provider(manifest) => Self::Provider(Arc::new(
                ProviderProcess::new(manifest.clone(), descriptor, changes),
            )),
        }
    }

    pub(super) fn enable(
        &self,
        root: &Path,
        manifest: &InstalledManifest,
        data: &Path,
        cancellation: &Cancellation,
    ) -> Result<(), SourceError> {
        match (self, manifest) {
            (Self::Catalog(module), InstalledManifest::Catalog(manifest)) => {
                module.enable(root, manifest, data, cancellation)
            }
            (Self::Provider(module), InstalledManifest::Provider(_)) => {
                module.enable(root, data, cancellation)
            }
            _ => Err(super::error("package_changed")),
        }
    }

    pub(super) fn disable(&self) {
        match self {
            Self::Catalog(module) => module.disable(),
            Self::Provider(module) => module.disable(),
        }
    }

    pub(super) fn shutdown(&self) -> Option<Arc<Transport>> {
        match self {
            Self::Catalog(module) => module.shutdown(),
            Self::Provider(module) => module.shutdown(),
        }
    }

    pub(super) fn busy(&self) -> bool {
        match self {
            Self::Catalog(module) => module.busy(),
            Self::Provider(module) => module.busy(),
        }
    }

    pub(super) fn check_health(&self) {
        match self {
            Self::Catalog(module) => module.check_health(),
            Self::Provider(module) => module.check_health(),
        }
    }
}

impl CatalogSource for Module {
    fn descriptor(&self) -> PluginDescriptor {
        match self {
            Self::Catalog(module) => module.descriptor(),
            Self::Provider(module) => module.descriptor(),
        }
    }

    fn generation(&self) -> u64 {
        match self {
            Self::Catalog(module) => module.generation(),
            Self::Provider(module) => module.generation(),
        }
    }

    fn catalog_page(
        &self,
        query: &CatalogQuery,
        cancellation: &Cancellation,
    ) -> Result<CatalogPage, SourceError> {
        match self {
            Self::Catalog(module) => module.catalog_page(query, cancellation),
            Self::Provider(module) => module.catalog_page(query, cancellation),
        }
    }
}
