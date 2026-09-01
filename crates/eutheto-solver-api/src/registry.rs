use crate::{CapabilityMatrix, SolverBackend};
use eutheto_types::BackendId;
use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

/// Immutable deterministic registry of explicitly supplied backend implementations.
pub struct SolverRegistry {
    matrix: CapabilityMatrix,
    backends: BTreeMap<BackendId, Arc<dyn SolverBackend>>,
}

impl SolverRegistry {
    /// Builds a registry only after descriptor and generated-matrix agreement is established.
    ///
    /// # Errors
    ///
    /// Returns an error if a backend descriptor is invalid, a backend ID is duplicated, a
    /// descriptor disagrees with the support matrix, or the registered and production backend
    /// sets differ.
    pub fn new(
        matrix: CapabilityMatrix,
        backends: impl IntoIterator<Item = Arc<dyn SolverBackend>>,
    ) -> Result<Self, RegistryError> {
        let mut registered = BTreeMap::new();
        for backend in backends {
            backend
                .descriptor()
                .validate()
                .map_err(|error| RegistryError::InvalidDescriptor(error.to_string()))?;
            let id = backend.descriptor().id.clone();
            if registered.contains_key(&id) {
                return Err(RegistryError::DuplicateBackend(id));
            }
            matrix
                .validate_descriptor(backend.descriptor())
                .map_err(|error| RegistryError::MatrixMismatch(error.to_string()))?;
            registered.insert(id, backend);
        }
        let matrix_ids: Vec<_> = matrix.production_backend_ids().cloned().collect();
        let registered_ids: Vec<_> = registered.keys().cloned().collect();
        if matrix_ids != registered_ids {
            return Err(RegistryError::RegistryMatrixSetMismatch);
        }
        Ok(Self {
            matrix,
            backends: registered,
        })
    }

    /// Phase-02 production registry. Generated production columns are authoritative and empty.
    ///
    /// # Errors
    ///
    /// Returns an error if the generated support matrix is invalid or its production backend set
    /// cannot form a valid registry.
    pub fn production() -> Result<Self, RegistryError> {
        let matrix = CapabilityMatrix::generated()
            .map_err(|error| RegistryError::MatrixMismatch(error.to_string()))?;
        Self::new(matrix, Vec::new())
    }

    #[must_use]
    pub fn matrix(&self) -> &CapabilityMatrix {
        &self.matrix
    }

    #[must_use]
    pub fn get(&self, id: &BackendId) -> Option<&Arc<dyn SolverBackend>> {
        self.backends.get(id)
    }

    #[must_use]
    pub fn descriptors(
        &self,
    ) -> impl ExactSizeIterator<Item = &crate::SolverDescriptor> + DoubleEndedIterator {
        self.backends.values().map(|backend| backend.descriptor())
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.backends.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.backends.len()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RegistryError {
    InvalidDescriptor(String),
    DuplicateBackend(BackendId),
    MatrixMismatch(String),
    RegistryMatrixSetMismatch,
}

impl fmt::Display for RegistryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "solver registry error: {self:?}")
    }
}

impl std::error::Error for RegistryError {}
