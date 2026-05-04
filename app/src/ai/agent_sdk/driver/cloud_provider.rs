use std::{collections::HashMap, ffi::OsString, future::Future, pin::Pin};

use anyhow::Error;
use warpui::ModelSpawner;

use super::terminal::TerminalDriver;

pub(crate) type Result<T> = std::result::Result<T, CloudProviderSetupError>;

#[derive(Debug, thiserror::Error)]
#[error("{provider_name} setup failed")]
pub(crate) struct CloudProviderSetupError {
    provider_name: &'static str,
    #[source]
    source: Error,
}

/// A cloud provider that we configure automatic Oz access to.
pub(crate) trait CloudProvider: Send {
    /// Return environment variables that should be injected into the terminal
    /// session.
    fn env_vars(&self) -> Result<HashMap<OsString, OsString>>;

    /// Perform any async setup that requires the terminal session to be running.
    fn setup(
        &mut self,
        _spawner: ModelSpawner<TerminalDriver>,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }

    /// Best-effort cleanup of any resources created during setup.
    ///
    /// The default implementation is a no-op.
    fn cleanup(self: Box<Self>) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> {
        Box::pin(async { Ok(()) })
    }
}

/// Collect all environment variables from a list of providers.
pub(crate) fn collect_env_vars(
    providers: &[Box<dyn CloudProvider>],
    vars: &mut HashMap<OsString, OsString>,
) -> Result<()> {
    for provider in providers {
        vars.extend(provider.env_vars()?);
    }
    Ok(())
}
