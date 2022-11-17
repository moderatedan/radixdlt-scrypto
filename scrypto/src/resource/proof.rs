use radix_engine_lib::data::ScryptoCustomTypeId;
use radix_engine_lib::engine::api::{EngineApi, SysNativeInvokable};
use radix_engine_lib::engine::types::{ProofId, RENodeId};
use radix_engine_lib::math::Decimal;
use radix_engine_lib::model::*;
use sbor::rust::collections::BTreeSet;
use sbor::rust::fmt::Debug;
use sbor::rust::vec::Vec;
use sbor::*;
use scrypto::engine::scrypto_env::ScryptoEnv;
use scrypto::scrypto_env_native_fn;

use crate::resource::*;
use crate::scrypto;

// TODO: Evaluate if we should have a ProofValidationModeBuilder to construct more complex validation modes.
/// Specifies the validation mode that should be used for validating a `Proof`.
pub enum ProofValidationMode {
    /// Specifies that the `Proof` should be validated against a single `ResourceAddress`.
    ValidateResourceAddress(ResourceAddress),

    /// Specifies that the `Proof` should have its resource address validated against a set of `ResourceAddress`es. If
    /// the `Proof`'s resource address belongs to the set, then its valid.
    ValidateResourceAddressBelongsTo(BTreeSet<ResourceAddress>),

    /// Specifies that the `Proof` should be validating for containing a specific `NonFungibleAddress`.
    ValidateContainsNonFungible(NonFungibleAddress),

    /// Specifies that the `Proof` should be validated against a single resource address and a set of `NonFungibleId`s
    /// to ensure that the `Proof` contains all of the NonFungibles in the set.
    ValidateContainsNonFungibles(ResourceAddress, BTreeSet<NonFungibleId>),

    /// Specifies that the `Proof` should be validated for the amount of resources that it contains.
    ValidateContainsAmount(ResourceAddress, Decimal),
}

impl From<ResourceAddress> for ProofValidationMode {
    fn from(resource_address: ResourceAddress) -> Self {
        Self::ValidateResourceAddress(resource_address)
    }
}

impl From<NonFungibleAddress> for ProofValidationMode {
    fn from(non_fungible_address: NonFungibleAddress) -> Self {
        Self::ValidateContainsNonFungible(non_fungible_address)
    }
}

pub trait SysProof {
    fn sys_clone<Y, E: Debug + TypeId<ScryptoCustomTypeId> + Decode<ScryptoCustomTypeId>>(
        &self,
        sys_calls: &mut Y,
    ) -> Result<Proof, E>
    where
        Y: EngineApi<E> + SysNativeInvokable<ProofCloneInvocation, E>;
    fn sys_drop<Y, E: Debug + TypeId<ScryptoCustomTypeId> + Decode<ScryptoCustomTypeId>>(
        self,
        sys_calls: &mut Y,
    ) -> Result<(), E>
    where
        Y: EngineApi<E>;
}

impl SysProof for Proof {
    fn sys_clone<Y, E: Debug + TypeId<ScryptoCustomTypeId> + Decode<ScryptoCustomTypeId>>(
        &self,
        sys_calls: &mut Y,
    ) -> Result<Proof, E>
    where
        Y: EngineApi<E> + SysNativeInvokable<ProofCloneInvocation, E>,
    {
        sys_calls.sys_invoke(ProofCloneInvocation { receiver: self.0 })
    }

    fn sys_drop<Y, E: Debug + TypeId<ScryptoCustomTypeId> + Decode<ScryptoCustomTypeId>>(
        self,
        sys_calls: &mut Y,
    ) -> Result<(), E>
    where
        Y: EngineApi<E>,
    {
        sys_calls.sys_drop_node(RENodeId::Proof(self.0))
    }
}

pub trait ScryptoProof: Sized {
    fn clone(&self) -> Self;
    fn validate_proof<T>(
        self,
        validation_mode: T,
    ) -> Result<ValidatedProof, (Self, ProofValidationError)>
    where
        T: Into<ProofValidationMode>;
    fn unsafe_skip_proof_validation(self) -> ValidatedProof;
    fn from_validated_proof(validated_proof: ValidatedProof) -> Self;
    fn validate(&self, validation_mode: ProofValidationMode) -> Result<(), ProofValidationError>;
    fn validate_resource_address(
        &self,
        resource_address: ResourceAddress,
    ) -> Result<(), ProofValidationError>;
    fn validate_resource_address_belongs_to(
        &self,
        resource_addresses: &BTreeSet<ResourceAddress>,
    ) -> Result<(), ProofValidationError>;
    fn validate_contains_non_fungible_id(
        &self,
        non_fungible_id: NonFungibleId,
    ) -> Result<(), ProofValidationError>;
    fn validate_contains_non_fungible_ids(
        &self,
        expected_non_fungible_ids: &BTreeSet<NonFungibleId>,
    ) -> Result<(), ProofValidationError>;
    fn validate_contains_amount(&self, amount: Decimal) -> Result<(), ProofValidationError>;
    fn amount(&self) -> Decimal;
    fn non_fungible_ids(&self) -> BTreeSet<NonFungibleId>;
    fn resource_address(&self) -> ResourceAddress;
    fn drop(self);
}

impl ScryptoProof for Proof {
    fn clone(&self) -> Self {
        Self(self.sys_clone(&mut ScryptoEnv).unwrap().0)
    }

    /// Validates a `Proof`'s resource address creating a `ValidatedProof` if the validation succeeds.
    ///
    /// This method takes ownership of the proof and validates that its resource address matches that expected by the
    /// caller. If the validation is successful, then a `ValidatedProof` is returned, otherwise, a `ValidateProofError`
    /// is returned.
    ///
    /// # Example:
    ///
    /// ```ignore
    /// let proof: Proof = bucket.create_proof();
    /// match proof.validate_proof(admin_badge_resource_address) {
    ///     Ok(validated_proof) => {
    ///         info!(
    ///             "Validation successful. Proof has a resource address of {} and amount of {}",
    ///             validated_proof.resource_address(),
    ///             validated_proof.amount(),
    ///         );
    ///     },
    ///     Err(error) => {
    ///         info!("Error validating proof: {:?}", error);
    ///     },
    /// }
    /// ```
    fn validate_proof<T>(
        self,
        validation_mode: T,
    ) -> Result<ValidatedProof, (Self, ProofValidationError)>
    where
        T: Into<ProofValidationMode>,
    {
        let validation_mode: ProofValidationMode = validation_mode.into();
        match self.validate(validation_mode) {
            Ok(()) => Ok(ValidatedProof(self)),
            Err(error) => Err((self, error)),
        }
    }

    /// Skips the validation process of the proof producing a validated proof **WITHOUT** performing any validation.
    ///
    /// # WARNING:
    ///
    /// This method skips the validation of the resource address of the proof. Therefore, the data, or `NonFungibleId`
    /// of of the returned `ValidatedProof` should **NOT** be trusted as the proof could potentially belong to any
    /// resource address. If you call this method, you should perform your own validation.
    fn unsafe_skip_proof_validation(self) -> ValidatedProof {
        ValidatedProof(self)
    }

    /// Converts a `ValidatedProof` into a `Proof`.
    fn from_validated_proof(validated_proof: ValidatedProof) -> Self {
        validated_proof.into()
    }

    fn validate(&self, validation_mode: ProofValidationMode) -> Result<(), ProofValidationError> {
        match validation_mode {
            ProofValidationMode::ValidateResourceAddress(resource_address) => {
                self.validate_resource_address(resource_address)?;
                Ok(())
            }
            ProofValidationMode::ValidateResourceAddressBelongsTo(resource_addresses) => {
                self.validate_resource_address_belongs_to(&resource_addresses)?;
                Ok(())
            }
            ProofValidationMode::ValidateContainsNonFungible(non_fungible_address) => {
                self.validate_resource_address(non_fungible_address.resource_address())?;
                self.validate_contains_non_fungible_id(non_fungible_address.non_fungible_id())?;
                Ok(())
            }
            ProofValidationMode::ValidateContainsNonFungibles(
                resource_address,
                non_fungible_ids,
            ) => {
                self.validate_resource_address(resource_address)?;
                self.validate_contains_non_fungible_ids(&non_fungible_ids)?;
                Ok(())
            }
            ProofValidationMode::ValidateContainsAmount(resource_address, amount) => {
                self.validate_resource_address(resource_address)?;
                self.validate_contains_amount(amount)?;
                Ok(())
            }
        }
    }

    fn validate_resource_address(
        &self,
        resource_address: ResourceAddress,
    ) -> Result<(), ProofValidationError> {
        if self.resource_address() == resource_address {
            Ok(())
        } else {
            Err(ProofValidationError::InvalidResourceAddress(
                resource_address,
            ))
        }
    }

    fn validate_resource_address_belongs_to(
        &self,
        resource_addresses: &BTreeSet<ResourceAddress>,
    ) -> Result<(), ProofValidationError> {
        if resource_addresses.contains(&self.resource_address()) {
            Ok(())
        } else {
            Err(ProofValidationError::ResourceAddressDoesNotBelongToList)
        }
    }

    fn validate_contains_non_fungible_id(
        &self,
        non_fungible_id: NonFungibleId,
    ) -> Result<(), ProofValidationError> {
        if self.non_fungible_ids().get(&non_fungible_id).is_some() {
            Ok(())
        } else {
            Err(ProofValidationError::NonFungibleIdNotFound)
        }
    }

    fn validate_contains_non_fungible_ids(
        &self,
        expected_non_fungible_ids: &BTreeSet<NonFungibleId>,
    ) -> Result<(), ProofValidationError> {
        let actual_non_fungible_ids = self.non_fungible_ids();
        let contains_all_non_fungible_ids = expected_non_fungible_ids
            .iter()
            .all(|non_fungible_id| actual_non_fungible_ids.get(non_fungible_id).is_some());
        if contains_all_non_fungible_ids {
            Ok(())
        } else {
            Err(ProofValidationError::NonFungibleIdNotFound)
        }
    }

    fn validate_contains_amount(&self, amount: Decimal) -> Result<(), ProofValidationError> {
        if self.amount() >= amount {
            Ok(())
        } else {
            Err(ProofValidationError::InvalidAmount(amount))
        }
    }

    scrypto_env_native_fn! {
        fn amount(&self) -> Decimal {
            ProofGetAmountInvocation {
                receiver: self.0
            }
        }
        fn non_fungible_ids(&self) -> BTreeSet<NonFungibleId> {
            ProofGetNonFungibleIdsInvocation {
                receiver: self.0
            }
        }
        fn resource_address(&self) -> ResourceAddress {
            ProofGetResourceAddressInvocation {
                receiver: self.0
            }
        }
    }

    fn drop(self) {
        self.sys_drop(&mut ScryptoEnv).unwrap()
    }
}

/// Represents a proof of owning some resource that has had its resource address validated.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct ValidatedProof(pub(crate) Proof);

#[cfg(target_arch = "wasm32")]
impl Clone for ValidatedProof {
    fn clone(&self) -> Self {
        ValidatedProof(self.0.clone())
    }
}

impl ValidatedProof {
    scrypto_env_native_fn! {
        pub fn amount(&self) -> Decimal {
            ProofGetAmountInvocation {
                receiver: self.proof_id(),
            }
        }
        pub fn non_fungible_ids(&self) -> BTreeSet<NonFungibleId> {
            ProofGetNonFungibleIdsInvocation {
                receiver: self.proof_id(),
            }
        }
        pub fn resource_address(&self) -> ResourceAddress {
            ProofGetResourceAddressInvocation {
                receiver: self.proof_id(),
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn drop(self) {
        self.0.drop()
    }

    /// Whether this proof includes an ownership proof of any of the given resource.
    pub fn contains(&self, resource_address: ResourceAddress) -> bool {
        self.resource_address() == resource_address
    }

    /// Whether this proof includes an ownership proof of at least the given amount of resource.
    pub fn contains_resource(&self, amount: Decimal, resource_address: ResourceAddress) -> bool {
        self.resource_address() == resource_address && self.amount() > amount
    }

    /// Whether this proof includes an ownership proof of the given non-fungible.
    pub fn contains_non_fungible(&self, non_fungible_address: &NonFungibleAddress) -> bool {
        if self.resource_address() != non_fungible_address.resource_address() {
            return false;
        }

        self.non_fungible_ids()
            .iter()
            .any(|k| k.eq(&non_fungible_address.non_fungible_id()))
    }

    /// Returns all the non-fungible units contained.
    ///
    /// # Panics
    /// Panics if this is not a non-fungible proof.
    pub fn non_fungibles<T: NonFungibleData>(&self) -> Vec<NonFungible<T>> {
        let resource_address = self.resource_address();
        self.non_fungible_ids()
            .iter()
            .map(|id| NonFungible::from(NonFungibleAddress::new(resource_address, id.clone())))
            .collect()
    }

    /// Returns a singleton non-fungible id
    ///
    /// # Panics
    /// Panics if this is not a singleton bucket
    pub fn non_fungible_id(&self) -> NonFungibleId {
        let non_fungible_ids = self.non_fungible_ids();
        if non_fungible_ids.len() != 1 {
            panic!("Expecting singleton NFT vault");
        }
        self.non_fungible_ids().into_iter().next().unwrap()
    }

    /// Returns a singleton non-fungible.
    ///
    /// # Panics
    /// Panics if this is not a singleton proof
    pub fn non_fungible<T: NonFungibleData>(&self) -> NonFungible<T> {
        let non_fungibles = self.non_fungibles();
        if non_fungibles.len() != 1 {
            panic!("Expecting singleton NFT proof");
        }
        non_fungibles.into_iter().next().unwrap()
    }

    /// Checks if the referenced bucket is empty.
    pub fn is_empty(&self) -> bool {
        self.amount() == 0.into()
    }

    fn proof_id(&self) -> ProofId {
        self.0 .0
    }
}

impl Into<Proof> for ValidatedProof {
    fn into(self) -> Proof {
        self.0
    }
}
