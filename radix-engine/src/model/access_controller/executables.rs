use super::state_machine::*;
use super::*;
use crate::engine::{deref_and_update, ApplicationError, Executor, LockFlags, RENodeInit};
use crate::engine::{
    CallFrameUpdate, ExecutableInvocation, ResolvedActor, ResolverApi, RuntimeError, SystemApi,
};
use crate::model::{AccessRulesChainSubstate, GlobalAddressSubstate};
use crate::wasm::WasmEngine;
use native_sdk::resource::{SysBucket, Vault};
use radix_engine_interface::api::types::*;
use radix_engine_interface::constants::{CLOCK, PACKAGE_TOKEN};
use radix_engine_interface::data::scrypto_encode;
use radix_engine_interface::*;
use radix_engine_interface::{api::*, rule};

#[derive(Debug, Clone, PartialEq, Eq, ScryptoCategorize, ScryptoEncode, ScryptoDecode)]
pub enum AccessControllerError {
    OperationNotAllowedWhenPrimaryIsLocked,
    RecoveryForThisProposerAlreadyExists { proposer: Proposer },
    NoValidProposedRuleSetExists,
    TimeOverflow,
    TimedRecoveryDelayHasNotElapsed,
    TimedRecoveryCanNotBePerformedWhileDisabled,

    InvalidStateTransition,
}

impl From<AccessControllerError> for RuntimeError {
    fn from(value: AccessControllerError) -> Self {
        RuntimeError::ApplicationError(ApplicationError::AccessControllerError(value))
    }
}

//=================================
// Access Controller Create Global
//=================================

impl ExecutableInvocation for AccessControllerCreateGlobalInvocation {
    type Exec = Self;

    fn resolve<D: ResolverApi>(
        self,
        _deref: &mut D,
    ) -> Result<(ResolvedActor, CallFrameUpdate, Self::Exec), RuntimeError>
    where
        Self: Sized,
    {
        let actor =
            ResolvedActor::function(NativeFn::AccessController(AccessControllerFn::CreateGlobal));
        let call_frame_update = CallFrameUpdate::move_node(RENodeId::Bucket(self.controlled_asset));

        Ok((actor, call_frame_update, self))
    }
}

impl Executor for AccessControllerCreateGlobalInvocation {
    type Output = ComponentAddress;

    fn execute<Y, W: WasmEngine>(
        self,
        api: &mut Y,
    ) -> Result<(Self::Output, CallFrameUpdate), RuntimeError>
    where
        Y: SystemApi + EngineApi<RuntimeError> + InvokableModel<RuntimeError>,
    {
        // Creating a new vault and putting in it the controlled asset
        let vault = {
            let controlled_asset = Bucket(self.controlled_asset);

            let mut vault = controlled_asset
                .sys_resource_address(api)
                .and_then(|resource_address| Vault::sys_new(resource_address, api))?;
            vault.sys_put(controlled_asset, api)?;

            vault
        };

        // Constructing the Access Controller RENode and Substates
        let access_controller = RENodeInit::AccessController(
            AccessControllerSubstate::new(vault.0, self.timed_recovery_delay_in_minutes),
            AccessRulesChainSubstate {
                access_rules_chain: [access_rules_from_rule_set(self.rule_set)].into(),
            },
        );

        // Allocating an RENodeId and creating the access controller RENode
        let node_id = api.allocate_node_id(RENodeType::AccessController)?;
        api.create_node(node_id, access_controller)?;

        // Creating a global component address for the access controller RENode
        let global_node_id = api.allocate_node_id(RENodeType::GlobalAccessController)?;
        api.create_node(
            global_node_id,
            RENodeInit::Global(GlobalAddressSubstate::AccessController(node_id.into())),
        )?;

        Ok((global_node_id.into(), CallFrameUpdate::empty()))
    }
}

//================================
// Access Controller Create Proof
//================================

#[derive(Debug, Clone, Eq, PartialEq, ScryptoCategorize, ScryptoEncode, ScryptoDecode)]
pub struct AccessControllerCreateProofExecutable {
    pub receiver: RENodeId,
}

impl ExecutableInvocation for AccessControllerCreateProofInvocation {
    type Exec = AccessControllerCreateProofExecutable;

    fn resolve<D: ResolverApi>(
        self,
        deref: &mut D,
    ) -> Result<(ResolvedActor, CallFrameUpdate, Self::Exec), RuntimeError>
    where
        Self: Sized,
    {
        let mut call_frame_update = CallFrameUpdate::empty();
        let receiver = RENodeId::Global(GlobalAddress::Component(self.receiver));
        let resolved_receiver = deref_and_update(receiver, &mut call_frame_update, deref)?;

        let actor = ResolvedActor::method(
            NativeFn::AccessController(AccessControllerFn::CreateProof),
            resolved_receiver,
        );

        let executor = Self::Exec {
            receiver: resolved_receiver.receiver,
        };

        Ok((actor, call_frame_update, executor))
    }
}

impl Executor for AccessControllerCreateProofExecutable {
    type Output = Proof;

    fn execute<Y, W: WasmEngine>(
        self,
        api: &mut Y,
    ) -> Result<(Self::Output, CallFrameUpdate), RuntimeError>
    where
        Y: SystemApi + EngineApi<RuntimeError> + InvokableModel<RuntimeError>,
    {
        let proof = transition(
            self.receiver,
            api,
            AccessControllerCreateProofStateMachineInput,
        )?;
        let call_frame_update = CallFrameUpdate::move_node(RENodeId::Proof(proof.0));

        Ok((proof, call_frame_update))
    }
}

//=====================================
// Access Controller Initiate Recovery
//=====================================

#[derive(Debug, Clone, Eq, PartialEq, ScryptoCategorize, ScryptoEncode, ScryptoDecode)]
pub struct AccessControllerInitiateRecoveryExecutable {
    pub receiver: RENodeId,
    pub rule_set: RuleSet,
    pub proposer: Proposer,
    pub timed_recovery_delay_in_minutes: Option<u32>,
}

impl ExecutableInvocation for AccessControllerInitiateRecoveryAsPrimaryInvocation {
    type Exec = AccessControllerInitiateRecoveryExecutable;

    fn resolve<D: ResolverApi>(
        self,
        deref: &mut D,
    ) -> Result<(ResolvedActor, CallFrameUpdate, Self::Exec), RuntimeError>
    where
        Self: Sized,
    {
        let mut call_frame_update = CallFrameUpdate::empty();
        call_frame_update.add_ref(RENodeId::Global(GlobalAddress::Component(CLOCK)));

        let receiver = RENodeId::Global(GlobalAddress::Component(self.receiver));
        let resolved_receiver = deref_and_update(receiver, &mut call_frame_update, deref)?;

        let actor = ResolvedActor::method(
            NativeFn::AccessController(AccessControllerFn::InitiateRecoveryAsPrimary),
            resolved_receiver,
        );

        let executor = Self::Exec {
            receiver: resolved_receiver.receiver,
            rule_set: self.rule_set,
            proposer: Proposer::Primary,
            timed_recovery_delay_in_minutes: self.timed_recovery_delay_in_minutes,
        };

        Ok((actor, call_frame_update, executor))
    }
}

impl ExecutableInvocation for AccessControllerInitiateRecoveryAsRecoveryInvocation {
    type Exec = AccessControllerInitiateRecoveryExecutable;

    fn resolve<D: ResolverApi>(
        self,
        deref: &mut D,
    ) -> Result<(ResolvedActor, CallFrameUpdate, Self::Exec), RuntimeError>
    where
        Self: Sized,
    {
        let mut call_frame_update = CallFrameUpdate::empty();
        call_frame_update.add_ref(RENodeId::Global(GlobalAddress::Component(CLOCK)));

        let receiver = RENodeId::Global(GlobalAddress::Component(self.receiver));
        let resolved_receiver = deref_and_update(receiver, &mut call_frame_update, deref)?;

        let actor = ResolvedActor::method(
            NativeFn::AccessController(AccessControllerFn::InitiateRecoveryAsRecovery),
            resolved_receiver,
        );

        let executor = Self::Exec {
            receiver: resolved_receiver.receiver,
            rule_set: self.rule_set,
            proposer: Proposer::Recovery,
            timed_recovery_delay_in_minutes: self.timed_recovery_delay_in_minutes,
        };

        Ok((actor, call_frame_update, executor))
    }
}

impl Executor for AccessControllerInitiateRecoveryExecutable {
    type Output = ();

    fn execute<Y, W: WasmEngine>(
        self,
        api: &mut Y,
    ) -> Result<(Self::Output, CallFrameUpdate), RuntimeError>
    where
        Y: SystemApi + EngineApi<RuntimeError> + InvokableModel<RuntimeError>,
    {
        transition_mut(
            self.receiver,
            api,
            AccessControllerInitiateRecoveryStateMachineInput {
                proposer: self.proposer,
                rule_set: self.rule_set,
                timed_recovery_delay_in_minutes: self.timed_recovery_delay_in_minutes,
            },
        )?;

        Ok(((), CallFrameUpdate::empty()))
    }
}

//==========================================
// Access Controller Quick Confirm Recovery
//==========================================

#[derive(Debug, Clone, Eq, PartialEq, ScryptoCategorize, ScryptoEncode, ScryptoDecode)]
pub struct AccessControllerQuickConfirmRecoveryExecutable {
    pub receiver: RENodeId,
    pub rule_set: RuleSet,
    pub proposer: Proposer,
    pub confirmor: Role,
    pub timed_recovery_delay_in_minutes: Option<u32>,
}

impl ExecutableInvocation for AccessControllerQuickConfirmRecoveryAsPrimaryInvocation {
    type Exec = AccessControllerQuickConfirmRecoveryExecutable;

    fn resolve<D: ResolverApi>(
        self,
        deref: &mut D,
    ) -> Result<(ResolvedActor, CallFrameUpdate, Self::Exec), RuntimeError>
    where
        Self: Sized,
    {
        let mut call_frame_update = CallFrameUpdate::empty();
        let receiver = RENodeId::Global(GlobalAddress::Component(self.receiver));
        let resolved_receiver = deref_and_update(receiver, &mut call_frame_update, deref)?;

        let actor = ResolvedActor::method(
            NativeFn::AccessController(AccessControllerFn::QuickConfirmRecoveryAsPrimary),
            resolved_receiver,
        );

        let executor = Self::Exec {
            receiver: resolved_receiver.receiver,
            rule_set: self.rule_set,
            proposer: self.proposer,
            confirmor: Role::Primary,
            timed_recovery_delay_in_minutes: self.timed_recovery_delay_in_minutes,
        };

        Ok((actor, call_frame_update, executor))
    }
}

impl ExecutableInvocation for AccessControllerQuickConfirmRecoveryAsRecoveryInvocation {
    type Exec = AccessControllerQuickConfirmRecoveryExecutable;

    fn resolve<D: ResolverApi>(
        self,
        deref: &mut D,
    ) -> Result<(ResolvedActor, CallFrameUpdate, Self::Exec), RuntimeError>
    where
        Self: Sized,
    {
        let mut call_frame_update = CallFrameUpdate::empty();
        let receiver = RENodeId::Global(GlobalAddress::Component(self.receiver));
        let resolved_receiver = deref_and_update(receiver, &mut call_frame_update, deref)?;

        let actor = ResolvedActor::method(
            NativeFn::AccessController(AccessControllerFn::QuickConfirmRecoveryAsRecovery),
            resolved_receiver,
        );

        let executor = Self::Exec {
            receiver: resolved_receiver.receiver,
            rule_set: self.rule_set,
            proposer: self.proposer,
            confirmor: Role::Recovery,
            timed_recovery_delay_in_minutes: self.timed_recovery_delay_in_minutes,
        };

        Ok((actor, call_frame_update, executor))
    }
}

impl ExecutableInvocation for AccessControllerQuickConfirmRecoveryAsConfirmationInvocation {
    type Exec = AccessControllerQuickConfirmRecoveryExecutable;

    fn resolve<D: ResolverApi>(
        self,
        deref: &mut D,
    ) -> Result<(ResolvedActor, CallFrameUpdate, Self::Exec), RuntimeError>
    where
        Self: Sized,
    {
        let mut call_frame_update = CallFrameUpdate::empty();
        let receiver = RENodeId::Global(GlobalAddress::Component(self.receiver));
        let resolved_receiver = deref_and_update(receiver, &mut call_frame_update, deref)?;

        let actor = ResolvedActor::method(
            NativeFn::AccessController(AccessControllerFn::QuickConfirmRecoveryAsConfirmation),
            resolved_receiver,
        );

        let executor = Self::Exec {
            receiver: resolved_receiver.receiver,
            rule_set: self.rule_set,
            proposer: self.proposer,
            confirmor: Role::Confirmation,
            timed_recovery_delay_in_minutes: self.timed_recovery_delay_in_minutes,
        };

        Ok((actor, call_frame_update, executor))
    }
}

impl Executor for AccessControllerQuickConfirmRecoveryExecutable {
    type Output = ();

    fn execute<Y, W: WasmEngine>(
        self,
        api: &mut Y,
    ) -> Result<(Self::Output, CallFrameUpdate), RuntimeError>
    where
        Y: SystemApi + EngineApi<RuntimeError> + InvokableModel<RuntimeError>,
    {
        let recovery_proposal = transition_mut(
            self.receiver,
            api,
            AccessControllerQuickConfirmRecoveryStateMachineInput {
                rule_set: self.rule_set,
                confirmor: self.confirmor,
                proposer: self.proposer,
                timed_recovery_delay_in_minutes: self.timed_recovery_delay_in_minutes,
            },
        )?;

        // Update the access rules
        let new_access_rules = access_rules_from_rule_set(recovery_proposal.rule_set);
        for (group_name, access_rule) in new_access_rules.get_all_grouped_auth().iter() {
            api.invoke(AccessRulesSetGroupAccessRuleInvocation {
                receiver: self.receiver,
                index: 0,
                name: group_name.into(),
                rule: access_rule.clone(),
            })?;
        }

        Ok(((), CallFrameUpdate::empty()))
    }
}

//==========================================
// Access Controller Timed Confirm Recovery
//==========================================

#[derive(Debug, Clone, Eq, PartialEq, ScryptoCategorize, ScryptoEncode, ScryptoDecode)]
pub struct AccessControllerTimedConfirmRecoveryExecutable {
    pub receiver: RENodeId,
    pub rule_set: RuleSet,
    pub timed_recovery_delay_in_minutes: Option<u32>,
}

impl ExecutableInvocation for AccessControllerTimedConfirmRecoveryInvocation {
    type Exec = AccessControllerTimedConfirmRecoveryExecutable;

    fn resolve<D: ResolverApi>(
        self,
        deref: &mut D,
    ) -> Result<(ResolvedActor, CallFrameUpdate, Self::Exec), RuntimeError>
    where
        Self: Sized,
    {
        let mut call_frame_update = CallFrameUpdate::empty();
        call_frame_update.add_ref(RENodeId::Global(GlobalAddress::Component(CLOCK)));
        let receiver = RENodeId::Global(GlobalAddress::Component(self.receiver));
        let resolved_receiver = deref_and_update(receiver, &mut call_frame_update, deref)?;

        let actor = ResolvedActor::method(
            NativeFn::AccessController(AccessControllerFn::TimedConfirmRecovery),
            resolved_receiver,
        );

        let executor = Self::Exec {
            receiver: resolved_receiver.receiver,
            rule_set: self.rule_set,
            timed_recovery_delay_in_minutes: self.timed_recovery_delay_in_minutes,
        };

        Ok((actor, call_frame_update, executor))
    }
}

impl Executor for AccessControllerTimedConfirmRecoveryExecutable {
    type Output = ();

    fn execute<Y, W: WasmEngine>(
        self,
        api: &mut Y,
    ) -> Result<(Self::Output, CallFrameUpdate), RuntimeError>
    where
        Y: SystemApi + EngineApi<RuntimeError> + InvokableModel<RuntimeError>,
    {
        let recovery_proposal = transition_mut(
            self.receiver,
            api,
            AccessControllerTimedConfirmRecoveryStateMachineInput {
                rule_set: self.rule_set,
                timed_recovery_delay_in_minutes: self.timed_recovery_delay_in_minutes,
            },
        )?;

        // Update the access rules
        let new_access_rules = access_rules_from_rule_set(recovery_proposal.rule_set);
        for (group_name, access_rule) in new_access_rules.get_all_grouped_auth().iter() {
            api.invoke(AccessRulesSetGroupAccessRuleInvocation {
                receiver: self.receiver,
                index: 0,
                name: group_name.into(),
                rule: access_rule.clone(),
            })?;
        }

        Ok(((), CallFrameUpdate::empty()))
    }
}

//===========================================
// Access Controller Cancel Recovery Attempt
//===========================================

#[derive(Debug, Clone, Eq, PartialEq, ScryptoCategorize, ScryptoEncode, ScryptoDecode)]
pub struct AccessControllerCancelRecoveryAttemptExecutable {
    pub receiver: RENodeId,
    pub rule_set: RuleSet,
    pub proposer: Proposer,
    pub timed_recovery_delay_in_minutes: Option<u32>,
}

impl ExecutableInvocation for AccessControllerCancelRecoveryAttemptAsPrimaryInvocation {
    type Exec = AccessControllerCancelRecoveryAttemptExecutable;

    fn resolve<D: ResolverApi>(
        self,
        deref: &mut D,
    ) -> Result<(ResolvedActor, CallFrameUpdate, Self::Exec), RuntimeError>
    where
        Self: Sized,
    {
        let mut call_frame_update = CallFrameUpdate::empty();
        let receiver = RENodeId::Global(GlobalAddress::Component(self.receiver));
        let resolved_receiver = deref_and_update(receiver, &mut call_frame_update, deref)?;

        let actor = ResolvedActor::method(
            NativeFn::AccessController(AccessControllerFn::CancelRecoveryAttemptAsPrimary),
            resolved_receiver,
        );

        let executor = Self::Exec {
            receiver: resolved_receiver.receiver,
            rule_set: self.rule_set,
            proposer: Proposer::Primary,
            timed_recovery_delay_in_minutes: self.timed_recovery_delay_in_minutes,
        };

        Ok((actor, call_frame_update, executor))
    }
}

impl ExecutableInvocation for AccessControllerCancelRecoveryAttemptAsRecoveryInvocation {
    type Exec = AccessControllerCancelRecoveryAttemptExecutable;

    fn resolve<D: ResolverApi>(
        self,
        deref: &mut D,
    ) -> Result<(ResolvedActor, CallFrameUpdate, Self::Exec), RuntimeError>
    where
        Self: Sized,
    {
        let mut call_frame_update = CallFrameUpdate::empty();
        let receiver = RENodeId::Global(GlobalAddress::Component(self.receiver));
        let resolved_receiver = deref_and_update(receiver, &mut call_frame_update, deref)?;

        let actor = ResolvedActor::method(
            NativeFn::AccessController(AccessControllerFn::CancelRecoveryAttemptAsRecovery),
            resolved_receiver,
        );

        let executor = Self::Exec {
            receiver: resolved_receiver.receiver,
            rule_set: self.rule_set,
            proposer: Proposer::Recovery,
            timed_recovery_delay_in_minutes: self.timed_recovery_delay_in_minutes,
        };

        Ok((actor, call_frame_update, executor))
    }
}

impl Executor for AccessControllerCancelRecoveryAttemptExecutable {
    type Output = ();

    fn execute<Y, W: WasmEngine>(
        self,
        api: &mut Y,
    ) -> Result<(Self::Output, CallFrameUpdate), RuntimeError>
    where
        Y: SystemApi + EngineApi<RuntimeError> + InvokableModel<RuntimeError>,
    {
        transition_mut(
            self.receiver,
            api,
            AccessControllerCancelRecoveryAttemptStateMachineInput {
                proposer: self.proposer,
                rule_set: self.rule_set,
                timed_recovery_delay_in_minutes: self.timed_recovery_delay_in_minutes,
            },
        )?;

        Ok(((), CallFrameUpdate::empty()))
    }
}

//=====================================
// Access Controller Lock Primary Role
//=====================================

#[derive(Debug, Clone, Eq, PartialEq, ScryptoCategorize, ScryptoEncode, ScryptoDecode)]
pub struct AccessControllerLockPrimaryRoleExecutable {
    pub receiver: RENodeId,
}

impl ExecutableInvocation for AccessControllerLockPrimaryRoleInvocation {
    type Exec = AccessControllerLockPrimaryRoleExecutable;

    fn resolve<D: ResolverApi>(
        self,
        deref: &mut D,
    ) -> Result<(ResolvedActor, CallFrameUpdate, Self::Exec), RuntimeError>
    where
        Self: Sized,
    {
        let mut call_frame_update = CallFrameUpdate::empty();
        let receiver = RENodeId::Global(GlobalAddress::Component(self.receiver));
        let resolved_receiver = deref_and_update(receiver, &mut call_frame_update, deref)?;

        let actor = ResolvedActor::method(
            NativeFn::AccessController(AccessControllerFn::LockPrimaryRole),
            resolved_receiver,
        );

        let executor = Self::Exec {
            receiver: resolved_receiver.receiver,
        };

        Ok((actor, call_frame_update, executor))
    }
}

impl Executor for AccessControllerLockPrimaryRoleExecutable {
    type Output = ();

    fn execute<Y, W: WasmEngine>(
        self,
        api: &mut Y,
    ) -> Result<(Self::Output, CallFrameUpdate), RuntimeError>
    where
        Y: SystemApi + EngineApi<RuntimeError> + InvokableModel<RuntimeError>,
    {
        transition_mut(
            self.receiver,
            api,
            AccessControllerLockPrimaryRoleStateMachineInput,
        )?;

        Ok(((), CallFrameUpdate::empty()))
    }
}

//=======================================
// Access Controller Unlock Primary Role
//=======================================

#[derive(Debug, Clone, Eq, PartialEq, ScryptoCategorize, ScryptoEncode, ScryptoDecode)]
pub struct AccessControllerUnlockPrimaryRoleExecutable {
    pub receiver: RENodeId,
}

impl ExecutableInvocation for AccessControllerUnlockPrimaryRoleInvocation {
    type Exec = AccessControllerUnlockPrimaryRoleExecutable;

    fn resolve<D: ResolverApi>(
        self,
        deref: &mut D,
    ) -> Result<(ResolvedActor, CallFrameUpdate, Self::Exec), RuntimeError>
    where
        Self: Sized,
    {
        let mut call_frame_update = CallFrameUpdate::empty();
        let receiver = RENodeId::Global(GlobalAddress::Component(self.receiver));
        let resolved_receiver = deref_and_update(receiver, &mut call_frame_update, deref)?;

        let actor = ResolvedActor::method(
            NativeFn::AccessController(AccessControllerFn::UnlockPrimaryRole),
            resolved_receiver,
        );

        let executor = Self::Exec {
            receiver: resolved_receiver.receiver,
        };

        Ok((actor, call_frame_update, executor))
    }
}

impl Executor for AccessControllerUnlockPrimaryRoleExecutable {
    type Output = ();

    fn execute<Y, W: WasmEngine>(
        self,
        api: &mut Y,
    ) -> Result<(Self::Output, CallFrameUpdate), RuntimeError>
    where
        Y: SystemApi + EngineApi<RuntimeError> + InvokableModel<RuntimeError>,
    {
        transition_mut(
            self.receiver,
            api,
            AccessControllerUnlockPrimaryRoleStateMachineInput,
        )?;

        Ok(((), CallFrameUpdate::empty()))
    }
}

//=========
// Helpers
//=========

fn access_rules_from_rule_set(rule_set: RuleSet) -> AccessRules {
    let mut access_rules = AccessRules::new();

    // Primary Role Rules
    let primary_group = "primary";
    access_rules.set_group_access_rule(primary_group.into(), rule_set.primary_role.clone());
    access_rules.set_method_access_rule_to_group(
        AccessRuleKey::Native(NativeFn::AccessController(AccessControllerFn::CreateProof)),
        primary_group.into(),
    );
    access_rules.set_method_access_rule_to_group(
        AccessRuleKey::Native(NativeFn::AccessController(
            AccessControllerFn::InitiateRecoveryAsPrimary,
        )),
        primary_group.into(),
    );
    access_rules.set_method_access_rule_to_group(
        AccessRuleKey::Native(NativeFn::AccessController(
            AccessControllerFn::QuickConfirmRecoveryAsPrimary,
        )),
        primary_group.into(),
    );
    access_rules.set_method_access_rule_to_group(
        AccessRuleKey::Native(NativeFn::AccessController(
            AccessControllerFn::CancelRecoveryAttemptAsPrimary,
        )),
        primary_group.into(),
    );

    // Recovery Role Rules
    let recovery_group = "recovery";
    access_rules.set_group_access_rule(recovery_group.into(), rule_set.recovery_role.clone());
    access_rules.set_method_access_rule_to_group(
        AccessRuleKey::Native(NativeFn::AccessController(
            AccessControllerFn::InitiateRecoveryAsRecovery,
        )),
        recovery_group.into(),
    );
    access_rules.set_method_access_rule_to_group(
        AccessRuleKey::Native(NativeFn::AccessController(
            AccessControllerFn::QuickConfirmRecoveryAsRecovery,
        )),
        recovery_group.into(),
    );
    access_rules.set_method_access_rule_to_group(
        AccessRuleKey::Native(NativeFn::AccessController(
            AccessControllerFn::TimedConfirmRecovery,
        )),
        recovery_group.into(),
    );
    access_rules.set_method_access_rule_to_group(
        AccessRuleKey::Native(NativeFn::AccessController(
            AccessControllerFn::CancelRecoveryAttemptAsRecovery,
        )),
        recovery_group.into(),
    );
    access_rules.set_method_access_rule_to_group(
        AccessRuleKey::Native(NativeFn::AccessController(
            AccessControllerFn::LockPrimaryRole,
        )),
        recovery_group.into(),
    );
    access_rules.set_method_access_rule_to_group(
        AccessRuleKey::Native(NativeFn::AccessController(
            AccessControllerFn::UnlockPrimaryRole,
        )),
        recovery_group.into(),
    );

    // Confirmation Role Rules
    let confirmation_group = "confirmation";
    access_rules.set_group_access_rule(
        confirmation_group.into(),
        rule_set.confirmation_role.clone(),
    );
    access_rules.set_method_access_rule_to_group(
        AccessRuleKey::Native(NativeFn::AccessController(
            AccessControllerFn::QuickConfirmRecoveryAsConfirmation,
        )),
        confirmation_group.into(),
    );

    let non_fungible_local_id = NonFungibleLocalId::Bytes(
        scrypto_encode(&PackageIdentifier::Native(NativePackage::AccessController)).unwrap(),
    );
    let non_fungible_global_id = NonFungibleGlobalId::new(PACKAGE_TOKEN, non_fungible_local_id);

    access_rules.default(rule!(deny_all), rule!(require(non_fungible_global_id)))
}

fn transition<Y, I>(
    node_id: RENodeId,
    api: &mut Y,
    input: I,
) -> Result<<AccessControllerSubstate as Transition<I>>::Output, RuntimeError>
where
    Y: SystemApi + EngineApi<RuntimeError> + InvokableModel<RuntimeError>,
    AccessControllerSubstate: Transition<I>,
{
    let offset = SubstateOffset::AccessController(AccessControllerOffset::AccessController);
    let handle = api.lock_substate(node_id, offset, LockFlags::read_only())?;

    let access_controller_clone = {
        let substate = api.get_ref(handle)?;
        let access_controller = substate.access_controller();
        access_controller.clone()
    };

    let rtn = access_controller_clone.transition(api, input)?;

    api.drop_lock(handle)?;

    Ok(rtn)
}

fn transition_mut<Y, I>(
    node_id: RENodeId,
    api: &mut Y,
    input: I,
) -> Result<<AccessControllerSubstate as TransitionMut<I>>::Output, RuntimeError>
where
    Y: SystemApi + EngineApi<RuntimeError> + InvokableModel<RuntimeError>,
    AccessControllerSubstate: TransitionMut<I>,
{
    let offset = SubstateOffset::AccessController(AccessControllerOffset::AccessController);
    let handle = api.lock_substate(node_id, offset, LockFlags::MUTABLE)?;

    let mut access_controller_clone = {
        let substate = api.get_ref(handle)?;
        let access_controller = substate.access_controller();
        access_controller.clone()
    };

    let rtn = access_controller_clone.transition_mut(api, input)?;

    {
        let mut substate = api.get_ref_mut(handle)?;
        let access_controller = substate.access_controller();
        *access_controller = access_controller_clone
    }

    api.drop_lock(handle)?;

    Ok(rtn)
}
