use mpl_utils::{assert_derivation_with_bump, assert_signer, cmp_pubkeys};
use solana_program::{
    account_info::AccountInfo,
    entrypoint::ProgramResult,
    program::{invoke, invoke_signed},
    program_option::COption,
    pubkey::Pubkey,
};
use spl_token_2022::state::{Account, Mint as MintAccount};

use crate::{
    assertions::{
        assert_derivation, assert_keys_equal, assert_mint_authority_matches_mint, assert_owned_by,
    },
    error::MetadataError,
    instruction::{Context, Mint, MintArgs},
    pda::{find_token_record_account, EDITION, PREFIX},
    state::{Metadata, TokenMetadataAccount, TokenStandard},
    utils::{
        assert_token_program_matches_package, create_token_record_account, freeze, thaw,
        unpack_initialized, validate_token,
    },
};

/// Mints tokens from a mint account.
///
/// This instruction will also initialized the associated token account if it does not exist – in
/// this case the `token_owner` will be required. When minting `*NonFungible` assets, the `authority`
/// must be the update authority; in all other cases, it must be the mint authority from the mint
/// account.
pub fn mint<'a>(
    program_id: &Pubkey,
    accounts: &'a [AccountInfo<'a>],
    args: MintArgs,
) -> ProgramResult {
    let context = Mint::to_context(accounts)?;
    match args {
        MintArgs::V1 { .. } => mint_v1(program_id, context, args),
    }
}

pub fn mint_v1(program_id: &Pubkey, ctx: Context<Mint>, args: MintArgs) -> ProgramResult {
    // get the args for the instruction
    let MintArgs::V1 { amount, .. } = args;

    if amount == 0 {
        return Err(MetadataError::AmountMustBeGreaterThanZero.into());
    }

    // checks that we have the required signers
    assert_signer(ctx.accounts.authority_info)?;
    assert_signer(ctx.accounts.payer_info)?;

    // validates the accounts
    assert_owned_by(ctx.accounts.metadata_info, program_id)?;
    assert_derivation(
        program_id,
        ctx.accounts.metadata_info,
        &[
            PREFIX.as_bytes(),
            program_id.as_ref(),
            ctx.accounts.mint_info.key.as_ref(),
        ],
    )?;

    let metadata = Metadata::from_account_info(ctx.accounts.metadata_info)?;
    if metadata.mint != *ctx.accounts.mint_info.key {
        return Err(MetadataError::MintMismatch.into());
    }

    assert_token_program_matches_package(ctx.accounts.spl_token_program_info)?;
    assert_owned_by(
        ctx.accounts.mint_info,
        ctx.accounts.spl_token_program_info.key,
    )?;
    let mint = unpack_initialized::<MintAccount>(&ctx.accounts.mint_info.data.borrow())?;

    // validates the authority:
    // - NonFungible must have a "valid" master edition
    // - Fungible must have the authority as the mint_authority
    match metadata.token_standard {
        Some(TokenStandard::ProgrammableNonFungible) | Some(TokenStandard::NonFungible) => {
            // for NonFungible assets, the mint authority is the master edition
            if let Some(master_edition_info) = ctx.accounts.master_edition_info {
                assert_derivation(
                    program_id,
                    master_edition_info,
                    &[
                        PREFIX.as_bytes(),
                        program_id.as_ref(),
                        ctx.accounts.mint_info.key.as_ref(),
                        EDITION.as_bytes(),
                    ],
                )?;
            } else {
                return Err(MetadataError::MissingMasterEditionAccount.into());
            }

            if mint.supply > 0 || amount > 1 {
                return Err(MetadataError::EditionsMustHaveExactlyOneToken.into());
            }

            // authority must be the update_authority of the metadata account
            if !cmp_pubkeys(&metadata.update_authority, ctx.accounts.authority_info.key) {
                return Err(MetadataError::UpdateAuthorityIncorrect.into());
            }
        }
        _ => {
            assert_mint_authority_matches_mint(&mint.mint_authority, ctx.accounts.authority_info)?;
        }
    }

    // validates the token account

    if ctx.accounts.token_info.data_is_empty() {
        // if we are initializing a new account, we need the token_owner
        let token_owner_info = if let Some(token_owner_info) = ctx.accounts.token_owner_info {
            token_owner_info
        } else {
            return Err(MetadataError::MissingTokenOwnerAccount.into());
        };

        // creating the associated token account
        invoke(
            &spl_associated_token_account::instruction::create_associated_token_account(
                ctx.accounts.payer_info.key,
                token_owner_info.key,
                ctx.accounts.mint_info.key,
                ctx.accounts.spl_token_program_info.key,
            ),
            &[
                ctx.accounts.payer_info.clone(),
                token_owner_info.clone(),
                ctx.accounts.mint_info.clone(),
                ctx.accounts.token_info.clone(),
            ],
        )?;
    } else {
        let token = validate_token(
            ctx.accounts.mint_info,
            ctx.accounts.token_info,
            ctx.accounts.token_owner_info,
            ctx.accounts.spl_token_program_info,
            metadata.token_standard,
            None, // we already checked the supply of the mint account
        )?;

        // validates that the close authority on the token is either None
        // or the master edition account for programmable assets

        if matches!(
            metadata.token_standard,
            Some(TokenStandard::ProgrammableNonFungible)
                | Some(TokenStandard::ProgrammableNonFungibleEdition)
        ) {
            if let COption::Some(close_authority) = token.close_authority {
                // the close authority must match the master edition if there is one set
                // on the token account
                if let Some(master_edition) = ctx.accounts.master_edition_info {
                    if close_authority != *master_edition.key {
                        return Err(MetadataError::InvalidCloseAuthority.into());
                    }
                } else {
                    return Err(MetadataError::MissingMasterEditionAccount.into());
                };
            }
        }
    }

    let token = unpack_initialized::<Account>(&ctx.accounts.token_info.data.borrow())?;

    match metadata.token_standard {
        Some(TokenStandard::NonFungible) | Some(TokenStandard::ProgrammableNonFungible) => {
            // for pNFTs, we require the token record account
            if matches!(
                metadata.token_standard,
                Some(TokenStandard::ProgrammableNonFungible)
            ) {
                // we always need the token_record_info
                let token_record_info = ctx
                    .accounts
                    .token_record_info
                    .ok_or(MetadataError::MissingTokenRecord)?;

                let (pda_key, _) = find_token_record_account(
                    ctx.accounts.mint_info.key,
                    ctx.accounts.token_info.key,
                );
                // validates the derivation
                assert_keys_equal(&pda_key, token_record_info.key)?;

                if token_record_info.data_is_empty() {
                    create_token_record_account(
                        program_id,
                        token_record_info,
                        ctx.accounts.mint_info,
                        ctx.accounts.token_info,
                        ctx.accounts.payer_info,
                        ctx.accounts.system_program_info,
                    )?;
                } else {
                    assert_owned_by(token_record_info, &crate::ID)?;
                }
            }

            let master_edition_seeds = &[
                PREFIX.as_bytes(),
                program_id.as_ref(),
                ctx.accounts.mint_info.key.as_ref(),
                EDITION.as_bytes(),
                &[metadata
                    .edition_nonce
                    .ok_or(MetadataError::NotAMasterEdition)?],
            ];

            let master_edition_info = ctx
                .accounts
                .master_edition_info
                .ok_or(MetadataError::MissingMasterEditionAccount)?;

            assert_derivation_with_bump(
                &crate::ID,
                master_edition_info,
                master_edition_seeds,
                MetadataError::InvalidMasterEdition,
            )?;

            // thaw the token account for programmable assets; the account
            // is not frozen if we just initialized it
            if matches!(
                metadata.token_standard,
                Some(TokenStandard::ProgrammableNonFungible)
            ) && token.is_frozen()
            {
                thaw(
                    ctx.accounts.mint_info.clone(),
                    ctx.accounts.token_info.clone(),
                    master_edition_info.clone(),
                    ctx.accounts.spl_token_program_info.clone(),
                    metadata.edition_nonce,
                )?;
            }

            invoke_signed(
                &spl_token_2022::instruction::mint_to(
                    ctx.accounts.spl_token_program_info.key,
                    ctx.accounts.mint_info.key,
                    ctx.accounts.token_info.key,
                    master_edition_info.key,
                    &[],
                    amount,
                )?,
                &[
                    ctx.accounts.mint_info.clone(),
                    ctx.accounts.token_info.clone(),
                    master_edition_info.clone(),
                ],
                &[master_edition_seeds],
            )?;

            // programmable assets are always in a frozen state
            if matches!(
                metadata.token_standard,
                Some(TokenStandard::ProgrammableNonFungible)
            ) {
                freeze(
                    ctx.accounts.mint_info.clone(),
                    ctx.accounts.token_info.clone(),
                    master_edition_info.clone(),
                    ctx.accounts.spl_token_program_info.clone(),
                    metadata.edition_nonce,
                )?;
            }
        }
        _ => {
            invoke(
                &spl_token_2022::instruction::mint_to(
                    ctx.accounts.spl_token_program_info.key,
                    ctx.accounts.mint_info.key,
                    ctx.accounts.token_info.key,
                    ctx.accounts.authority_info.key,
                    &[],
                    amount,
                )?,
                &[
                    ctx.accounts.mint_info.clone(),
                    ctx.accounts.token_info.clone(),
                    ctx.accounts.authority_info.clone(),
                ],
            )?;
        }
    }

    Ok(())
}
