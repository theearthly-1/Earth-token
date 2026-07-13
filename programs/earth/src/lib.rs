use anchor_lang::prelude::*;
use anchor_spl::token_2022::{self, MintTo, Transfer, Token2022};
use anchor_spl::token_interface::{Mint, TokenAccount};

declare_id!("3aAndePxcwx5kgfGPK3gsAVPsi7PeXrikboMuVgLKkcg");

pub const ADMIN_AUTHORITY: Pubkey = solana_program::pubkey!("FndrmgjS9iZ7wgnj58fp49W3cMSc3XEfBYkYA8J4cTH3");
pub const BIRTH_ALLOCATION: u64 = 66_000_000_000;
pub const TOKEN_DECIMALS: u8 = 6;
pub const MINT_AUTHORITY_SEED: &[u8] = b"mint_authority";
pub const PROGRAM_STATE_SEED: &[u8] = b"program_state";
pub const VAULT_SEED: &[u8] = b"vault";
pub const HUMAN_REGISTRY_SEED: &[u8] = b"human_registry";
pub const PROPOSAL_SEED: &[u8] = b"proposal";
pub const VOTE_SEED: &[u8] = b"vote";
pub const QUORUM_THRESHOLD_BPS: u64 = 5100;
pub const VOTING_PERIOD: i64 = 604_800;

#[program]
pub mod earth {
    use super::*;

    pub fn initialize_mint(ctx: Context<InitializeMint>) -> Result<()> {
        require_keys_eq!(ctx.accounts.admin.key(), ADMIN_AUTHORITY, EarthError::UnauthorizedAdmin);
        let state = &mut ctx.accounts.program_state;
        state.admin_authority = ADMIN_AUTHORITY;
        state.mint = ctx.accounts.mint.key();
        state.mint_authority_bump = ctx.bumps.mint_authority;
        state.oracle_data_account = Pubkey::default();
        state.total_minted = 0;
        state.total_birth_events = 0;
        state.total_verified_humans = 0;
        state.total_proposals = 0;
        state.is_initialized = true;
        state.emergency_freeze = false;
        state.freeze_reason = [0u8; 64];
        state.freeze_timestamp = 0;
        Ok(())
    }

    pub fn register_human(ctx: Context<RegisterHuman>, iris_hash: [u8; 32]) -> Result<()> {
        let state = &ctx.accounts.program_state;
        require!(!state.emergency_freeze, EarthError::SystemFrozen);
        require!(state.oracle_data_account != Pubkey::default(), EarthError::OracleNotSet);
        require_keys_eq!(ctx.accounts.oracle_signer.key(), state.oracle_data_account, EarthError::UnauthorizedOracle);
        let human = &mut ctx.accounts.human_registry;
        require!(!human.is_registered, EarthError::HumanAlreadyRegistered);
        human.is_registered = true;
        human.iris_hash = iris_hash;
        human.wallet = ctx.accounts.human_wallet.key();
        human.registration_timestamp = Clock::get()?.unix_timestamp;
        human.is_active = true;
        human.has_voted_count = 0;
        let state = &mut ctx.accounts.program_state;
        state.total_verified_humans = state.total_verified_humans.checked_add(1).ok_or(EarthError::ArithmeticOverflow)?;
        Ok(())
    }

    pub fn transfer_with_human_check(ctx: Context<TransferWithHumanCheck>, amount: u64) -> Result<()> {
        require!(!ctx.accounts.program_state.emergency_freeze, EarthError::SystemFrozen);
        let cpi_accounts = Transfer {
            from: ctx.accounts.sender_token_account.to_account_info(),
            to: ctx.accounts.recipient_token_account.to_account_info(),
            authority: ctx.accounts.sender.to_account_info(),
        };
        token_2022::transfer(CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts), amount)?;
        Ok(())
    }

    pub fn create_proposal(ctx: Context<CreateProposal>, proposal_id: [u8; 32], proposal_type: ProposalType, description_hash: [u8; 32]) -> Result<()> {
        let state = &ctx.accounts.program_state;
        require!(!state.emergency_freeze, EarthError::SystemFrozen);
        require!(ctx.accounts.proposer_human_registry.is_registered, EarthError::ProposerNotHuman);
        require!(ctx.accounts.proposer_human_registry.is_active, EarthError::ProposerNotActive);
        let clock = Clock::get()?;
        let proposal = &mut ctx.accounts.proposal;
        proposal.proposal_id = proposal_id;
        proposal.proposal_type = proposal_type;
        proposal.description_hash = description_hash;
        proposal.proposer = ctx.accounts.proposer.key();
        proposal.created_at = clock.unix_timestamp;
        proposal.voting_ends_at = clock.unix_timestamp.checked_add(VOTING_PERIOD).ok_or(EarthError::ArithmeticOverflow)?;
        proposal.votes_for = 0;
        proposal.votes_against = 0;
        proposal.total_eligible_voters = state.total_verified_humans;
        proposal.is_active = true;
        proposal.is_executed = false;
        proposal.is_passed = false;
        let state = &mut ctx.accounts.program_state;
        state.total_proposals = state.total_proposals.checked_add(1).ok_or(EarthError::ArithmeticOverflow)?;
        Ok(())
    }

    pub fn cast_vote(ctx: Context<CastVote>, vote_choice: bool) -> Result<()> {
        require!(!ctx.accounts.program_state.emergency_freeze, EarthError::SystemFrozen);
        require!(ctx.accounts.voter_human_registry.is_registered, EarthError::VoterNotHuman);
        require!(ctx.accounts.voter_human_registry.is_active, EarthError::VoterNotActive);
        require_keys_eq!(ctx.accounts.voter_human_registry.wallet, ctx.accounts.voter.key(), EarthError::VoterWalletMismatch);
        let proposal = &mut ctx.accounts.proposal;
        require!(proposal.is_active, EarthError::ProposalNotActive);
        let clock = Clock::get()?;
        require!(clock.unix_timestamp <= proposal.voting_ends_at, EarthError::VotingPeriodEnded);
        let vote_record = &mut ctx.accounts.vote_record;
        require!(!vote_record.has_voted, EarthError::AlreadyVoted);
        vote_record.has_voted = true;
        vote_record.voter = ctx.accounts.voter.key();
        vote_record.proposal = proposal.proposal_id;
        vote_record.vote_choice = vote_choice;
        vote_record.voted_at = clock.unix_timestamp;
        if vote_choice {
            proposal.votes_for = proposal.votes_for.checked_add(1).ok_or(EarthError::ArithmeticOverflow)?;
        } else {
            proposal.votes_against = proposal.votes_against.checked_add(1).ok_or(EarthError::ArithmeticOverflow)?;
        }
        Ok(())
    }

    pub fn finalize_proposal(ctx: Context<FinalizeProposal>) -> Result<()> {
        require!(!ctx.accounts.program_state.emergency_freeze, EarthError::SystemFrozen);
        let proposal = &mut ctx.accounts.proposal;
        require!(proposal.is_active, EarthError::ProposalNotActive);
        require!(!proposal.is_executed, EarthError::ProposalAlreadyExecuted);
        let clock = Clock::get()?;
        require!(clock.unix_timestamp > proposal.voting_ends_at, EarthError::VotingPeriodNotEnded);
        require!(proposal.total_eligible_voters > 0, EarthError::NoEligibleVoters);
        let total_votes = proposal.votes_for.checked_add(proposal.votes_against).ok_or(EarthError::ArithmeticOverflow)?;
        let quorum_required = proposal.total_eligible_voters.checked_mul(QUORUM_THRESHOLD_BPS).ok_or(EarthError::ArithmeticOverflow)?.checked_div(10_000).ok_or(EarthError::ArithmeticOverflow)?;
        proposal.is_active = false;
        proposal.is_executed = true;
        proposal.is_passed = (total_votes >= quorum_required) && (proposal.votes_for > proposal.votes_against);
        Ok(())
    }

    pub fn emergency_freeze(ctx: Context<EmergencyFreeze>, reason: [u8; 64]) -> Result<()> {
        require_keys_eq!(ctx.accounts.admin.key(), ADMIN_AUTHORITY, EarthError::UnauthorizedAdmin);
        let state = &mut ctx.accounts.program_state;
        state.emergency_freeze = true;
        state.freeze_reason = reason;
        state.freeze_timestamp = Clock::get()?.unix_timestamp;
        Ok(())
    }

    pub fn emergency_unfreeze(ctx: Context<EmergencyUnfreeze>) -> Result<()> {
        require_keys_eq!(ctx.accounts.admin.key(), ADMIN_AUTHORITY, EarthError::UnauthorizedAdmin);
        let state = &mut ctx.accounts.program_state;
        require!(state.emergency_freeze, EarthError::SystemNotFrozen);
        let clock = Clock::get()?;
        require!(clock.unix_timestamp >= state.freeze_timestamp.checked_add(259_200).ok_or(EarthError::ArithmeticOverflow)?, EarthError::CoolingPeriodNotElapsed);
        require!(ctx.accounts.unfreeze_proposal.is_executed, EarthError::UnfreezeProposalNotExecuted);
        require!(ctx.accounts.unfreeze_proposal.is_passed, EarthError::UnfreezeProposalNotPassed);
        require!(ctx.accounts.unfreeze_proposal.proposal_type == ProposalType::UnfreezeSystem, EarthError::WrongProposalType);
        state.emergency_freeze = false;
        state.freeze_reason = [0u8; 64];
        state.freeze_timestamp = 0;
        Ok(())
    }

    pub fn update_oracle(ctx: Context<UpdateOracle>, new_oracle: Pubkey) -> Result<()> {
        require_keys_eq!(ctx.accounts.admin.key(), ADMIN_AUTHORITY, EarthError::UnauthorizedAdmin);
        require!(!ctx.accounts.program_state.emergency_freeze, EarthError::SystemFrozen);
        ctx.accounts.program_state.oracle_data_account = new_oracle;
        Ok(())
    }

    pub fn mint_birth_allocation(ctx: Context<MintBirthAllocation>, birth_event_id: [u8; 32], beneficiary: Pubkey, is_minor: bool, birth_timestamp: i64) -> Result<()> {
        let state = &ctx.accounts.program_state;
        require!(!state.emergency_freeze, EarthError::SystemFrozen);
        require!(state.oracle_data_account != Pubkey::default(), EarthError::OracleNotSet);
        require_keys_eq!(ctx.accounts.oracle_signer.key(), state.oracle_data_account, EarthError::UnauthorizedOracle);
        let vault = &mut ctx.accounts.vault_state;
        require!(!vault.is_initialized, EarthError::BirthEventAlreadyProcessed);
        vault.is_initialized = true;
        vault.birth_event_id = birth_event_id;
        vault.beneficiary = beneficiary;
        vault.is_minor = is_minor;
        vault.birth_timestamp = birth_timestamp;
        vault.amount = BIRTH_ALLOCATION;
        vault.is_claimed = false;
        vault.vault_token_account = ctx.accounts.vault_token_account.key();
        vault.vault_bump = ctx.bumps.vault_state;
        vault.unlock_timestamp = if is_minor { birth_timestamp.checked_add(568_036_800).ok_or(EarthError::ArithmeticOverflow)? } else { 0 };
        let bump = ctx.accounts.program_state.mint_authority_bump;
        let signer_seeds: &[&[&[u8]]] = &[&[MINT_AUTHORITY_SEED, &[bump]]];
        token_2022::mint_to(CpiContext::new_with_signer(ctx.accounts.token_program.to_account_info(), MintTo { mint: ctx.accounts.mint.to_account_info(), to: ctx.accounts.vault_token_account.to_account_info(), authority: ctx.accounts.mint_authority.to_account_info() }, signer_seeds), BIRTH_ALLOCATION)?;
        let state = &mut ctx.accounts.program_state;
        state.total_minted = state.total_minted.checked_add(BIRTH_ALLOCATION).ok_or(EarthError::ArithmeticOverflow)?;
        state.total_birth_events = state.total_birth_events.checked_add(1).ok_or(EarthError::ArithmeticOverflow)?;
        Ok(())
    }

    pub fn claim_vault(ctx: Context<ClaimVault>) -> Result<()> {
        require!(!ctx.accounts.program_state.emergency_freeze, EarthError::SystemFrozen);
        let vault = &mut ctx.accounts.vault_state;
        require!(vault.is_initialized, EarthError::VaultNotInitialized);
        require!(!vault.is_claimed, EarthError::VaultAlreadyClaimed);
        require_keys_eq!(ctx.accounts.beneficiary.key(), vault.beneficiary, EarthError::UnauthorizedBeneficiary);
        require!(ctx.accounts.beneficiary_human_registry.is_registered, EarthError::ClaimerNotHuman);
        require!(ctx.accounts.beneficiary_human_registry.is_active, EarthError::ClaimerNotActive);
        if vault.is_minor {
            require!(Clock::get()?.unix_timestamp >= vault.unlock_timestamp, EarthError::VaultTimeLocked);
        }
        let birth_event_id = vault.birth_event_id;
        let vault_bump = vault.vault_bump;
        let claim_amount = vault.amount;
        vault.is_claimed = true;
        let signer: &[&[&[u8]]] = &[&[VAULT_SEED, &birth_event_id, &[vault_bump]]];
        token_2022::transfer(CpiContext::new_with_signer(ctx.accounts.token_program.to_account_info(), Transfer { from: ctx.accounts.vault_token_account.to_account_info(), to: ctx.accounts.beneficiary_token_account.to_account_info(), authority: ctx.accounts.vault_state.to_account_info() }, signer), claim_amount)?;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct InitializeMint<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(init, payer = admin, mint::decimals = TOKEN_DECIMALS, mint::authority = mint_authority, mint::token_program = token_program)]
    pub mint: InterfaceAccount<'info, Mint>,
    /// CHECK: PDA mint authority
    #[account(seeds = [MINT_AUTHORITY_SEED], bump)]
    pub mint_authority: UncheckedAccount<'info>,
    #[account(init, payer = admin, space = 8 + ProgramState::INIT_SPACE, seeds = [PROGRAM_STATE_SEED], bump)]
    pub program_state: Account<'info, ProgramState>,
    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(iris_hash: [u8; 32])]
pub struct RegisterHuman<'info> {
    #[account(mut)]
    pub oracle_signer: Signer<'info>,
    /// CHECK: wallet being registered
    pub human_wallet: UncheckedAccount<'info>,
    #[account(init, payer = oracle_signer, space = 8 + HumanRegistry::INIT_SPACE, seeds = [HUMAN_REGISTRY_SEED, human_wallet.key().as_ref()], bump)]
    pub human_registry: Account<'info, HumanRegistry>,
    #[account(mut, seeds = [PROGRAM_STATE_SEED], bump, constraint = program_state.is_initialized @ EarthError::NotInitialized)]
    pub program_state: Account<'info, ProgramState>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct TransferWithHumanCheck<'info> {
    #[account(mut)]
    pub sender: Signer<'info>,
    /// CHECK: recipient wallet
    pub recipient_wallet: UncheckedAccount<'info>,
    #[account(constraint = sender_human_registry.is_registered @ EarthError::SenderNotHuman, constraint = sender_human_registry.wallet == sender.key() @ EarthError::SenderWalletMismatch, seeds = [HUMAN_REGISTRY_SEED, sender.key().as_ref()], bump)]
    pub sender_human_registry: Account<'info, HumanRegistry>,
    #[account(constraint = recipient_human_registry.is_registered @ EarthError::RecipientNotHuman, constraint = recipient_human_registry.wallet == recipient_wallet.key() @ EarthError::RecipientWalletMismatch, seeds = [HUMAN_REGISTRY_SEED, recipient_wallet.key().as_ref()], bump)]
    pub recipient_human_registry: Account<'info, HumanRegistry>,
    #[account(mut)]
    pub sender_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(mut)]
    pub recipient_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(seeds = [PROGRAM_STATE_SEED], bump, constraint = program_state.is_initialized @ EarthError::NotInitialized)]
    pub program_state: Account<'info, ProgramState>,
    pub token_program: Program<'info, Token2022>,
}

#[derive(Accounts)]
#[instruction(proposal_id: [u8; 32])]
pub struct CreateProposal<'info> {
    #[account(mut)]
    pub proposer: Signer<'info>,
    #[account(constraint = proposer_human_registry.is_registered @ EarthError::ProposerNotHuman, constraint = proposer_human_registry.wallet == proposer.key() @ EarthError::ProposerWalletMismatch, seeds = [HUMAN_REGISTRY_SEED, proposer.key().as_ref()], bump)]
    pub proposer_human_registry: Account<'info, HumanRegistry>,
    #[account(init, payer = proposer, space = 8 + Proposal::INIT_SPACE, seeds = [PROPOSAL_SEED, &proposal_id], bump)]
    pub proposal: Account<'info, Proposal>,
    #[account(mut, seeds = [PROGRAM_STATE_SEED], bump, constraint = program_state.is_initialized @ EarthError::NotInitialized)]
    pub program_state: Account<'info, ProgramState>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CastVote<'info> {
    #[account(mut)]
    pub voter: Signer<'info>,
    #[account(constraint = voter_human_registry.is_registered @ EarthError::VoterNotHuman, constraint = voter_human_registry.wallet == voter.key() @ EarthError::VoterWalletMismatch, seeds = [HUMAN_REGISTRY_SEED, voter.key().as_ref()], bump)]
    pub voter_human_registry: Account<'info, HumanRegistry>,
    #[account(mut, constraint = proposal.is_active @ EarthError::ProposalNotActive)]
    pub proposal: Account<'info, Proposal>,
    #[account(init, payer = voter, space = 8 + VoteRecord::INIT_SPACE, seeds = [VOTE_SEED, proposal.proposal_id.as_ref(), voter.key().as_ref()], bump)]
    pub vote_record: Account<'info, VoteRecord>,
    #[account(seeds = [PROGRAM_STATE_SEED], bump, constraint = program_state.is_initialized @ EarthError::NotInitialized)]
    pub program_state: Account<'info, ProgramState>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct FinalizeProposal<'info> {
    #[account(mut)]
    pub proposal: Account<'info, Proposal>,
    #[account(seeds = [PROGRAM_STATE_SEED], bump, constraint = program_state.is_initialized @ EarthError::NotInitialized)]
    pub program_state: Account<'info, ProgramState>,
}

#[derive(Accounts)]
pub struct EmergencyFreeze<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, seeds = [PROGRAM_STATE_SEED], bump, constraint = program_state.is_initialized @ EarthError::NotInitialized)]
    pub program_state: Account<'info, ProgramState>,
}

#[derive(Accounts)]
pub struct EmergencyUnfreeze<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, seeds = [PROGRAM_STATE_SEED], bump, constraint = program_state.is_initialized @ EarthError::NotInitialized)]
    pub program_state: Account<'info, ProgramState>,
    pub unfreeze_proposal: Account<'info, Proposal>,
}

#[derive(Accounts)]
pub struct UpdateOracle<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(mut, seeds = [PROGRAM_STATE_SEED], bump, constraint = program_state.is_initialized @ EarthError::NotInitialized)]
    pub program_state: Account<'info, ProgramState>,
}

#[derive(Accounts)]
#[instruction(birth_event_id: [u8; 32])]
pub struct MintBirthAllocation<'info> {
    #[account(mut)]
    pub oracle_signer: Signer<'info>,
    #[account(mut, constraint = mint.key() == program_state.mint @ EarthError::InvalidMint)]
    pub mint: InterfaceAccount<'info, Mint>,
    /// CHECK: PDA mint authority
    #[account(seeds = [MINT_AUTHORITY_SEED], bump = program_state.mint_authority_bump)]
    pub mint_authority: UncheckedAccount<'info>,
    #[account(mut, seeds = [PROGRAM_STATE_SEED], bump, constraint = program_state.is_initialized @ EarthError::NotInitialized)]
    pub program_state: Account<'info, ProgramState>,
    #[account(init, payer = oracle_signer, space = 8 + VaultState::INIT_SPACE, seeds = [VAULT_SEED, &birth_event_id], bump)]
    pub vault_state: Account<'info, VaultState>,
    #[account(init, payer = oracle_signer, token::mint = mint, token::authority = vault_state, token::token_program = token_program)]
    pub vault_token_account: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct ClaimVault<'info> {
    #[account(mut)]
    pub beneficiary: Signer<'info>,
    #[account(mut, constraint = vault_state.is_initialized @ EarthError::VaultNotInitialized, constraint = !vault_state.is_claimed @ EarthError::VaultAlreadyClaimed, constraint = vault_state.beneficiary == beneficiary.key() @ EarthError::UnauthorizedBeneficiary)]
    pub vault_state: Account<'info, VaultState>,
    #[account(mut, constraint = vault_token_account.key() == vault_state.vault_token_account @ EarthError::InvalidVaultTokenAccount)]
    pub vault_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(mut)]
    pub beneficiary_token_account: InterfaceAccount<'info, TokenAccount>,
    #[account(constraint = beneficiary_human_registry.is_registered @ EarthError::ClaimerNotHuman, constraint = beneficiary_human_registry.wallet == beneficiary.key() @ EarthError::ClaimerWalletMismatch, seeds = [HUMAN_REGISTRY_SEED, beneficiary.key().as_ref()], bump)]
    pub beneficiary_human_registry: Account<'info, HumanRegistry>,
    #[account(seeds = [PROGRAM_STATE_SEED], bump, constraint = program_state.is_initialized @ EarthError::NotInitialized)]
    pub program_state: Account<'info, ProgramState>,
    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
}

#[account]
#[derive(InitSpace)]
pub struct ProgramState {
    pub admin_authority: Pubkey,
    pub mint: Pubkey,
    pub mint_authority_bump: u8,
    pub oracle_data_account: Pubkey,
    pub total_minted: u64,
    pub total_birth_events: u64,
    pub total_verified_humans: u64,
    pub total_proposals: u64,
    pub is_initialized: bool,
    pub emergency_freeze: bool,
    pub freeze_reason: [u8; 64],
    pub freeze_timestamp: i64,
}

#[account]
#[derive(InitSpace)]
pub struct HumanRegistry {
    pub is_registered: bool,
    pub iris_hash: [u8; 32],
    pub wallet: Pubkey,
    pub registration_timestamp: i64,
    pub is_active: bool,
    pub has_voted_count: u64,
}

#[account]
#[derive(InitSpace)]
pub struct VaultState {
    pub is_initialized: bool,
    pub birth_event_id: [u8; 32],
    pub beneficiary: Pubkey,
    pub is_minor: bool,
    pub birth_timestamp: i64,
    pub unlock_timestamp: i64,
    pub amount: u64,
    pub is_claimed: bool,
    pub vault_token_account: Pubkey,
    pub vault_bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct Proposal {
    pub proposal_id: [u8; 32],
    pub proposal_type: ProposalType,
    pub description_hash: [u8; 32],
    pub proposer: Pubkey,
    pub created_at: i64,
    pub voting_ends_at: i64,
    pub votes_for: u64,
    pub votes_against: u64,
    pub total_eligible_voters: u64,
    pub is_active: bool,
    pub is_executed: bool,
    pub is_passed: bool,
}

#[account]
#[derive(InitSpace)]
pub struct VoteRecord {
    pub has_voted: bool,
    pub voter: Pubkey,
    pub proposal: [u8; 32],
    pub vote_choice: bool,
    pub voted_at: i64,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug, InitSpace)]
pub enum ProposalType {
    SystemChange,
    AllocationRelease,
    OracleUpdate,
    EmergencyFreeze,
    UnfreezeSystem,
    InfrastructureDeployment,
}

#[error_code]
pub enum EarthError {
    #[msg("Unauthorized: Signer is not the hardcoded admin authority.")]
    UnauthorizedAdmin,
    #[msg("Unauthorized: Oracle signer does not match authorized oracle data account.")]
    UnauthorizedOracle,
    #[msg("Program state has not been initialized.")]
    NotInitialized,
    #[msg("SYSTEM FROZEN: Emergency kill switch active. All operations halted.")]
    SystemFrozen,
    #[msg("System is not currently frozen.")]
    SystemNotFrozen,
    #[msg("72-hour cooling period has not elapsed since freeze.")]
    CoolingPeriodNotElapsed,
    #[msg("Unfreeze proposal has not been executed.")]
    UnfreezeProposalNotExecuted,
    #[msg("Unfreeze proposal did not pass consensus vote.")]
    UnfreezeProposalNotPassed,
    #[msg("Wrong proposal type for this operation.")]
    WrongProposalType,
    #[msg("Birth event has already been processed.")]
    BirthEventAlreadyProcessed,
    #[msg("Arithmetic overflow in calculation.")]
    ArithmeticOverflow,
    #[msg("Vault has not been initialized.")]
    VaultNotInitialized,
    #[msg("Vault has already been claimed.")]
    VaultAlreadyClaimed,
    #[msg("Unauthorized: Signer is not the vault beneficiary.")]
    UnauthorizedBeneficiary,
    #[msg("Vault is time-locked. Beneficiary must be 18+ to claim.")]
    VaultTimeLocked,
    #[msg("Invalid mint account provided.")]
    InvalidMint,
    #[msg("Invalid vault token account.")]
    InvalidVaultTokenAccount,
    #[msg("Human already registered in the network.")]
    HumanAlreadyRegistered,
    #[msg("Oracle has not been configured. Call update_oracle first.")]
    OracleNotSet,
    #[msg("No eligible voters were registered when this proposal was created.")]
    NoEligibleVoters,
    #[msg("AI BLOCK: Sender wallet is not registered as human-owned.")]
    SenderNotHuman,
    #[msg("Sender wallet is not active.")]
    SenderNotActive,
    #[msg("Sender wallet does not match human registry entry.")]
    SenderWalletMismatch,
    #[msg("AI BLOCK: Recipient wallet is not registered as human-owned.")]
    RecipientNotHuman,
    #[msg("Recipient wallet is not active.")]
    RecipientNotActive,
    #[msg("Recipient wallet does not match human registry entry.")]
    RecipientWalletMismatch,
    #[msg("AI BLOCK: Claimer is not registered as human.")]
    ClaimerNotHuman,
    #[msg("Claimer wallet is not active.")]
    ClaimerNotActive,
    #[msg("Claimer wallet does not match human registry entry.")]
    ClaimerWalletMismatch,
    #[msg("AI BLOCK: Voter is not registered as human.")]
    VoterNotHuman,
    #[msg("Voter wallet is not active.")]
    VoterNotActive,
    #[msg("Voter wallet does not match human registry entry.")]
    VoterWalletMismatch,
    #[msg("You have already voted on this proposal.")]
    AlreadyVoted,
    #[msg("Proposal is not currently active.")]
    ProposalNotActive,
    #[msg("Voting period has ended for this proposal.")]
    VotingPeriodEnded,
    #[msg("Voting period has not ended yet.")]
    VotingPeriodNotEnded,
    #[msg("Proposal has already been executed.")]
    ProposalAlreadyExecuted,
    #[msg("AI BLOCK: Proposer is not registered as human.")]
    ProposerNotHuman,
    #[msg("Proposer wallet is not active.")]
    ProposerNotActive,
    #[msg("Proposer wallet does not match human registry entry.")]
    ProposerWalletMismatch,
}
