use crate::component::{ComponentAddress, PackageAddress};
use crate::resource::*;
use crate::{address, construct_address};

// After changing Radix Engine ID allocation, you will most likely need to update the addresses below.
//
// To obtain the new addresses, uncomment the println code in `id_allocator.rs` and
// run `cd radix-engine && cargo test -- bootstrap_receipt_should_match_constants --nocapture`.
//
// We've arranged the addresses in the order they're created in the genesis transaction.

/// The address of the sys-faucet package.
pub const SYS_FAUCET_PACKAGE: PackageAddress = construct_address!(
    EntityType::Package,
    0, 44, 100, 204, 153, 17, 167, 139, 223, 159, 221, 222, 95, 90, 157, 196, 136, 236, 235, 197, 213, 35, 187, 15, 207, 158
);

/// The address of the account package.
pub const ACCOUNT_PACKAGE: PackageAddress = construct_address!(
    EntityType::Package,
    117, 149, 161, 192, 155, 192, 68, 56, 79, 186, 128, 155, 199, 188, 92, 59, 83, 241, 146, 178, 126, 213, 55, 167, 164, 201
);

/// The ECDSA virtual resource address.
pub const ECDSA_SECP256K1_TOKEN: ResourceAddress = construct_address!(
    EntityType::Resource,
    185, 23, 55, 238, 138, 77, 229, 157, 73, 218, 212, 13, 229, 86, 14, 87, 84, 70, 106, 200, 76, 245, 67, 46, 169, 93
);

/// The system token which allows access to system resources (e.g. setting epoch)
pub const SYSTEM_TOKEN: ResourceAddress = construct_address!(
    EntityType::Resource,
    237, 145, 0, 85, 29, 127, 174, 145, 234, 244, 19, 229, 10, 60, 90, 89, 248, 185, 106, 249, 241, 41, 120, 144, 168, 244
);

/// The XRD resource address.
pub const RADIX_TOKEN: ResourceAddress = address!(
    EntityType::Resource,
    146, 35, 6, 166, 209, 58, 246, 56, 102, 182, 136, 201, 16, 55, 25, 208, 75, 20, 192, 96, 188, 72, 153, 166, 19, 181
);

/// The address of the SysFaucet component
pub const SYS_FAUCET_COMPONENT: ComponentAddress = construct_address!(
    EntityType::NormalComponent,
    241, 88, 60, 234, 185, 86, 59, 118, 36, 26, 46, 225, 245, 4, 254, 227, 6, 207, 47, 230, 180, 123, 170, 4, 214, 11
);

pub const SYS_SYSTEM_COMPONENT: ComponentAddress = construct_address!(
    EntityType::SystemComponent,
    35, 78, 150, 173, 221, 245, 198, 37, 78, 106, 20, 17, 169, 73, 152, 133, 204, 145, 37, 125, 141, 154, 21, 174, 199, 75
);

/// The ED25519 virtual resource address.
pub const EDDSA_ED25519_TOKEN: ResourceAddress = address!(
    EntityType::Resource,
    87, 220, 4, 44, 216, 203, 145, 111, 54, 48, 2, 10, 31, 51, 124, 236, 90, 84, 207, 239, 164, 197, 8, 79, 190, 60
);
