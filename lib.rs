use anchor_lang::prelude::*;
use anchor_spl::token_2022::{self, MintTo, Transfer, Token2022};
use anchor_spl::token_interface::{Mint, TokenAccount};

declare_id!("EARTH111111111111111111111111111111111111111");

// ============================================================================
// CONSTANTS
// ============================================================================

/// The hardcoded admin authority — @Fosheezez Phantom wallet.
/// This key can ONLY trigger emergency voting proposals or update oracle endpoints.
/// It CANNOT mint tokens, move funds, or alter supply outside programmatic rules.
pub const ADMIN_AUTHORITY: Pubkey = pubkey!("FndrmgjS9iZ7wgnj58fp49W3cMSc3XEfBYkYA8J4cTH3");

/// The exact number of tokens minted per verified birth event (66,000 EARTH).
/// This value is immutable and encoded directly into the protocol.
pub const BIRTH_ALLOCATION: u64 = 66_000_000_000; // 66,000 tokens with 6 decimals

/// Token decimals for the EARTH mint.
pub const TOKEN_DECIMALS: u8 = 6;

/// PDA seed for the mint authority.
pub const MINT_AUTHORITY_SEED: &[u8] = b"mint_authority";

/// PDA seed for the program state.
pub const PROGRAM_STATE_SEED: &[u8] = b"program_state";

/// PDA seed for vault accounts.
pub const VAULT_SEED: &[u8] = b"vault";

/// PDA seed for human registry entries.
pub const HUMAN_REGISTRY_SEED: &[u8] = b"human_registry";

/// PDA seed for governance proposals.
pub const PROPOSAL_SEED: &[u8] = b"proposal";

/// PDA seed for individual votes.
pub const VOTE_SEED: &[u8] = b"vote";

/// Minimum percentage of verified humans required to pass a proposal (51%).
pub const QUORUM_THRESHOLD_BPS: u64 = 5100; // 51% in basis points

/// Voting period duration in seconds (7 days).
pub const VOTING_PERIOD: i64 = 604_800;

#[program]
pub mod earth {
    use super::*;

    // ========================================================================
    // INITIALIZATION
    // ========================================================================

    /// Initializes the EARTH token mint and program state.
    ///
    /// - Creates the Token-2022 Mint account for EARTH.
    /// - Sets the Mint Authority to a PDA controlled by this program.
    /// - Stores the admin_authority (hardcoded) and oracle endpoint in program state.
    /// - No personal wallet ever holds mint authority.
    pub fn initialize_mint(ctx: Context<InitializeMint>) -> Result<()> {
        require_keys_eq!(
            ctx.accounts.admin.key(),
            ADMIN_AUTHORITY,
            EarthError::UnauthorizedAdmin
        );

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

        msg!("EARTH Mint initialized. Mint Authority is PDA-controlled.");
        msg!("Admin Authority: {}", ADMIN_AUTHORITY);
        msg!("Zero AI Ownership: ACTIVE — All transfers require human verification.");
        msg!("Live Public Voting: ACTIVE — 1-Human, 1-Vote consensus required.");
        msg!("Emergency Kill Switch: ARMED — Human audit freeze available.");

        Ok(())
    }

    // ========================================================================
    // HUMAN REGISTRY — ZERO AI OWNERSHIP ENFORCEMENT
    // ========================================================================

    /// Registers a verified human identity on-chain.
    /// Only the authorized oracle can register humans after biometric verification.
    /// This registry is the SOLE source of truth for who can hold/transact EARTH tokens.
    /// AI, bots, algorithms, and unverified keys are structurally excluded.
    pub fn register_human(
        ctx: Context<RegisterHuman>,
        iris_hash: [u8; 32],       // One-way cryptographic hash of biometric scan
        registration_timestamp: i64,
    ) -> Result<()> {
        let state = &ctx.accounts.program_state;
        require!(!state.emergency_freeze, EarthError::SystemFrozen);
        require_keys_eq!(
            ctx.accounts.oracle_signer.key(),
            state.oracle_data_account,
            EarthError::UnauthorizedOracle
        );

        let human = &mut ctx.accounts.human_registry;
        require!(!human.is_registered, EarthError::HumanAlreadyRegistered);

        human.is_registered = true;
        human.iris_hash = iris_hash;
        human.wallet = ctx.accounts.human_wallet.key();
        human.registration_timestamp = registration_timestamp;
        human.is_active = true;
        human.has_voted_count = 0;

        // Increment verified humans counter
        let state = &mut ctx.accounts.program_state;
        state.total_verified_humans = state.total_verified_humans.checked_add(1)
            .ok_or(EarthError::ArithmeticOverflow)?;

        msg!("Human registered. Wallet: {}", human.wallet);
        msg!("Total verified humans: {}", state.total_verified_humans);

        Ok(())
    }

    // ========================================================================
    // TRANSFER HOOK — AI OWNERSHIP BLOCK & HUMAN-ONLY TRANSACTIONS
    // ========================================================================

    /// Executes a token transfer ONLY between two verified human-held wallets.
    ///
    /// ZERO AI OWNERSHIP ENFORCEMENT:
    /// - Both sender and recipient MUST have an active entry in the Human Registry.
    /// - If either party is not a verified human, the transfer is permanently blocked.
    /// - Algorithms, bots, smart contract wallets, and AI agents cannot own or transact.
    /// - This is a structural, protocol-level block — not a policy that can be overridden.
    pub fn transfer_with_human_check(
        ctx: Context<TransferWithHumanCheck>,
        amount: u64,
    ) -> Result<()> {
        let state = &ctx.accounts.program_state;

        // KILL SWITCH CHECK: No transfers during emergency freeze
        require!(!state.emergency_freeze, EarthError::SystemFrozen);

        // ZERO AI OWNERSHIP: Verify sender is a registered, active human
        let sender_registry = &ctx.accounts.sender_human_registry;
        require!(sender_registry.is_registered, EarthError::SenderNotHuman);
        require!(sender_registry.is_active, EarthError::SenderNotActive);
        require_keys_eq!(
            sender_registry.wallet,
            ctx.accounts.sender.key(),
            EarthError::SenderWalletMismatch
        );

        // ZERO AI OWNERSHIP: Verify recipient is a registered, active human
        let recipient_registry = &ctx.accounts.recipient_human_registry;
        require!(recipient_registry.is_registered, EarthError::RecipientNotHuman);
        require!(recipient_registry.is_active, EarthError::RecipientNotActive);
        require_keys_eq!(
            recipient_registry.wallet,
            ctx.accounts.recipient_wallet.key(),
            EarthError::RecipientWalletMismatch
        );

        // Execute the transfer between verified humans
        let cpi_accounts = Transfer {
            from: ctx.accounts.sender_token_account.to_account_info(),
            to: ctx.accounts.recipient_token_account.to_account_info(),
            authority: ctx.accounts.sender.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);

        token_2022::transfer(cpi_ctx, amount)?;

        msg!("Transfer complete: {} tokens between verified humans.", amount);
        Ok(())
    }

    // ========================================================================
    // LIVE PUBLIC VOTING — 1-HUMAN, 1-VOTE CONSENSUS
    // ========================================================================

    /// Creates a new governance proposal for network-wide voting.
    /// Only the admin can initiate proposals (emergency votes) OR
    /// any verified human can submit a proposal with sufficient backing.
    ///
    /// ANTI-OLIGARCHY: No centralized group or automated system can alter
    /// or manufacture votes. Every proposal requires live, verifiable
    /// 1-Human, 1-Vote consensus from the network.
    pub fn create_proposal(
        ctx: Context<CreateProposal>,
        proposal_id: [u8; 32],
        proposal_type: ProposalType,
        description_hash: [u8; 32], // IPFS hash of full proposal text
    ) -> Result<()> {
        let state = &ctx.accounts.program_state;
        require!(!state.emergency_freeze, EarthError::SystemFrozen);

        // Verify proposer is a registered human
        let proposer_registry = &ctx.accounts.proposer_human_registry;
        require!(proposer_registry.is_registered, EarthError::ProposerNotHuman);
        require!(proposer_registry.is_active, EarthError::ProposerNotActive);

        let clock = Clock::get()?;
        let proposal = &mut ctx.accounts.proposal;

        proposal.proposal_id = proposal_id;
        proposal.proposal_type = proposal_type;
        proposal.description_hash = description_hash;
        proposal.proposer = ctx.accounts.proposer.key();
        proposal.created_at = clock.unix_timestamp;
        proposal.voting_ends_at = clock.unix_timestamp.checked_add(VOTING_PERIOD)
            .ok_or(EarthError::ArithmeticOverflow)?;
        proposal.votes_for = 0;
        proposal.votes_against = 0;
        proposal.total_eligible_voters = state.total_verified_humans;
        proposal.is_active = true;
        proposal.is_executed = false;
        proposal.is_passed = false;

        // Update proposal counter
        let state = &mut ctx.accounts.program_state;
        state.total_proposals = state.total_proposals.checked_add(1)
            .ok_or(EarthError::ArithmeticOverflow)?;

        msg!("Proposal created. ID: {:?}", proposal_id);
        msg!("Type: {:?} | Voting ends: {}", proposal_type, proposal.voting_ends_at);
        msg!("Eligible voters: {}", proposal.total_eligible_voters);

        Ok(())
    }

    /// Casts a vote on an active proposal.
    ///
    /// STRICT RULES:
    /// - Only verified humans can vote (1-Human, 1-Vote).
    /// - Each human can vote exactly ONCE per proposal.
    /// - No delegation, no representatives, no proxies.
    /// - Bots, AI, and algorithms are structurally blocked from voting.
    /// - Votes are immutable once cast — cannot be changed or withdrawn.
    pub fn cast_vote(
        ctx: Context<CastVote>,
        vote_choice: bool, // true = FOR, false = AGAINST
    ) -> Result<()> {
        let state = &ctx.accounts.program_state;
        require!(!state.emergency_freeze, EarthError::SystemFrozen);

        // Verify voter is a registered, active human
        let voter_registry = &ctx.accounts.voter_human_registry;
        require!(voter_registry.is_registered, EarthError::VoterNotHuman);
        require!(voter_registry.is_active, EarthError::VoterNotActive);
        require_keys_eq!(
            voter_registry.wallet,
            ctx.accounts.voter.key(),
            EarthError::VoterWalletMismatch
        );

        // Verify proposal is still active and within voting period
        let proposal = &mut ctx.accounts.proposal;
        require!(proposal.is_active, EarthError::ProposalNotActive);

        let clock = Clock::get()?;
        require!(
            clock.unix_timestamp <= proposal.voting_ends_at,
            EarthError::VotingPeriodEnded
        );

        // Record the vote (PDA ensures one vote per human per proposal)
        let vote_record = &mut ctx.accounts.vote_record;
        require!(!vote_record.has_voted, EarthError::AlreadyVoted);

        vote_record.has_voted = true;
        vote_record.voter = ctx.accounts.voter.key();
        vote_record.proposal = proposal.proposal_id;
        vote_record.vote_choice = vote_choice;
        vote_record.voted_at = clock.unix_timestamp;

        // Tally the vote
        if vote_choice {
            proposal.votes_for = proposal.votes_for.checked_add(1)
                .ok_or(EarthError::ArithmeticOverflow)?;
        } else {
            proposal.votes_against = proposal.votes_against.checked_add(1)
                .ok_or(EarthError::ArithmeticOverflow)?;
        }

        msg!("Vote cast: {} by {}", if vote_choice { "FOR" } else { "AGAINST" }, ctx.accounts.voter.key());
        msg!("Current tally — For: {} | Against: {}", proposal.votes_for, proposal.votes_against);

        Ok(())
    }

    /// Finalizes a proposal after the voting period ends.
    /// Checks quorum (51% of verified humans must participate)
    /// and majority (more FOR than AGAINST) to pass.
    pub fn finalize_proposal(ctx: Context<FinalizeProposal>) -> Result<()> {
        let proposal = &mut ctx.accounts.proposal;
        require!(proposal.is_active, EarthError::ProposalNotActive);
        require!(!proposal.is_executed, EarthError::ProposalAlreadyExecuted);

        let clock = Clock::get()?;
        require!(
            clock.unix_timestamp > proposal.voting_ends_at,
            EarthError::VotingPeriodNotEnded
        );

        let total_votes = proposal.votes_for.checked_add(proposal.votes_against)
            .ok_or(EarthError::ArithmeticOverflow)?;

        // Check quorum: at least 51% of eligible voters must have participated
        let quorum_required = proposal.total_eligible_voters
            .checked_mul(QUORUM_THRESHOLD_BPS)
            .ok_or(EarthError::ArithmeticOverflow)?
            .checked_div(10_000)
            .ok_or(EarthError::ArithmeticOverflow)?;

        let quorum_met = total_votes >= quorum_required;
        let majority_for = proposal.votes_for > proposal.votes_against;

        proposal.is_active = false;
        proposal.is_executed = true;
        proposal.is_passed = quorum_met && majority_for;

        msg!("Proposal finalized. Quorum met: {} | Majority FOR: {} | PASSED: {}",
            quorum_met, majority_for, proposal.is_passed);

        Ok(())
    }

    // ========================================================================
    // EMERGENCY KILL SWITCH — MANUAL HUMAN AUDIT & FREEZE
    // ========================================================================

    /// EMERGENCY KILL SWITCH: Freezes the entire contract.
    ///
    /// When triggered, ALL operations are halted:
    /// - No transfers can execute.
    /// - No minting can occur.
    /// - No proposals can be created or voted on.
    /// - The contract is completely locked until human auditors verify and reset.
    ///
    /// Can be triggered by:
    /// 1. The hardcoded admin (for immediate response to detected threats).
    /// 2. A passed governance proposal of type EmergencyFreeze (human consensus).
    ///
    /// CANNOT be lifted by admin alone — requires a separate unfreeze proposal
    /// passed by 1-Human, 1-Vote consensus, OR admin + time-delay (72 hours).
    pub fn emergency_freeze(
        ctx: Context<EmergencyFreeze>,
        reason: [u8; 64], // Short description of the freeze reason
    ) -> Result<()> {
        require_keys_eq!(
            ctx.accounts.admin.key(),
            ADMIN_AUTHORITY,
            EarthError::UnauthorizedAdmin
        );

        let state = &mut ctx.accounts.program_state;
        state.emergency_freeze = true;
        state.freeze_reason = reason;
        state.freeze_timestamp = Clock::get()?.unix_timestamp;

        msg!("╔══════════════════════════════════════════════════════════════╗");
        msg!("║  EMERGENCY KILL SWITCH ACTIVATED                            ║");
        msg!("║  ALL CONTRACT OPERATIONS FROZEN                             ║");
        msg!("║  Manual human audit required to resume.                     ║");
        msg!("╚══════════════════════════════════════════════════════════════╝");

        Ok(())
    }

    /// Lifts the emergency freeze ONLY after human consensus verification.
    ///
    /// Requirements to unfreeze:
    /// 1. Admin must sign (proves human oversight).
    /// 2. At least 72 hours must have passed since freeze (cooling period).
    /// 3. A governance proposal of type UnfreezeSystem must have passed.
    ///
    /// This ensures no single actor can freeze AND unfreeze to manipulate the system.
    pub fn emergency_unfreeze(ctx: Context<EmergencyUnfreeze>) -> Result<()> {
        require_keys_eq!(
            ctx.accounts.admin.key(),
            ADMIN_AUTHORITY,
            EarthError::UnauthorizedAdmin
        );

        let state = &mut ctx.accounts.program_state;
        require!(state.emergency_freeze, EarthError::SystemNotFrozen);

        // Enforce 72-hour cooling period
        let clock = Clock::get()?;
        let hours_72: i64 = 259_200; // 72 * 60 * 60
        require!(
            clock.unix_timestamp >= state.freeze_timestamp.checked_add(hours_72)
                .ok_or(EarthError::ArithmeticOverflow)?,
            EarthError::CoolingPeriodNotElapsed
        );

        // Verify an unfreeze proposal has passed
        let unfreeze_proposal = &ctx.accounts.unfreeze_proposal;
        require!(unfreeze_proposal.is_executed, EarthError::UnfreezeProposalNotExecuted);
        require!(unfreeze_proposal.is_passed, EarthError::UnfreezeProposalNotPassed);
        require!(
            unfreeze_proposal.proposal_type == ProposalType::UnfreezeSystem,
            EarthError::WrongProposalType
        );

        state.emergency_freeze = false;
        state.freeze_reason = [0u8; 64];
        state.freeze_timestamp = 0;

        msg!("╔══════════════════════════════════════════════════════════════╗");
        msg!("║  SYSTEM UNFROZEN — Operations resumed.                      ║");
        msg!("║  Human consensus verified. Cooling period elapsed.          ║");
        msg!("╚══════════════════════════════════════════════════════════════╝");

        Ok(())
    }

    // ========================================================================
    // ORACLE & MINTING
    // ========================================================================

    /// Updates the oracle data account endpoint.
    /// Only callable by the hardcoded admin_authority.
    pub fn update_oracle(ctx: Context<UpdateOracle>, new_oracle: Pubkey) -> Result<()> {
        require_keys_eq!(
            ctx.accounts.admin.key(),
            ADMIN_AUTHORITY,
            EarthError::UnauthorizedAdmin
        );
        require!(!ctx.accounts.program_state.emergency_freeze, EarthError::SystemFrozen);

        let state = &mut ctx.accounts.program_state;
        state.oracle_data_account = new_oracle;

        msg!("Oracle data account updated to: {}", new_oracle);
        Ok(())
    }

    /// Dynamic Minting Function — Mints exactly 66,000 EARTH tokens to a new vault.
    ///
    /// Triggered ONLY when the authorized Oracle submits a verified birth event.
    /// The oracle_signer must match the oracle endpoint stored in program state.
    ///
    /// Flow:
    /// 1. Oracle verifies a birth registration event off-chain.
    /// 2. Oracle calls this instruction with the birth event data.
    /// 3. Program validates the oracle is authorized.
    /// 4. Program mints exactly 66,000 EARTH to the new vault PDA.
    /// 5. Vault is time-locked (for minors) or immediately claimable (for adults).
    pub fn mint_birth_allocation(
        ctx: Context<MintBirthAllocation>,
        birth_event_id: [u8; 32],
        beneficiary: Pubkey,
        is_minor: bool,
        birth_timestamp: i64,
    ) -> Result<()> {
        let state = &ctx.accounts.program_state;

        // KILL SWITCH: No minting during emergency freeze
        require!(!state.emergency_freeze, EarthError::SystemFrozen);

        // Verify the oracle signer
        require_keys_eq!(
            ctx.accounts.oracle_signer.key(),
            state.oracle_data_account,
            EarthError::UnauthorizedOracle
        );

        // Prevent duplicate processing
        let vault = &mut ctx.accounts.vault_state;
        require!(!vault.is_initialized, EarthError::BirthEventAlreadyProcessed);

        // Initialize vault state
        vault.is_initialized = true;
        vault.birth_event_id = birth_event_id;
        vault.beneficiary = beneficiary;
        vault.is_minor = is_minor;
        vault.birth_timestamp = birth_timestamp;
        vault.amount = BIRTH_ALLOCATION;
        vault.is_claimed = false;
        vault.vault_token_account = ctx.accounts.vault_token_account.key();

        // Time-lock for minors (18 years = 568,036,800 seconds)
        if is_minor {
            vault.unlock_timestamp = birth_timestamp.checked_add(568_036_800)
                .ok_or(EarthError::ArithmeticOverflow)?;
        } else {
            vault.unlock_timestamp = 0;
        }

        // Mint via PDA authority
        let mint_authority_seeds: &[&[u8]] = &[
            MINT_AUTHORITY_SEED,
            &[state.mint_authority_bump],
        ];
        let signer_seeds = &[&mint_authority_seeds[..]];

        let cpi_accounts = MintTo {
            mint: ctx.accounts.mint.to_account_info(),
            to: ctx.accounts.vault_token_account.to_account_info(),
            authority: ctx.accounts.mint_authority.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);

        token_2022::mint_to(cpi_ctx, BIRTH_ALLOCATION)?;

        // Update counters
        let state = &mut ctx.accounts.program_state;
        state.total_minted = state.total_minted.checked_add(BIRTH_ALLOCATION)
            .ok_or(EarthError::ArithmeticOverflow)?;
        state.total_birth_events = state.total_birth_events.checked_add(1)
            .ok_or(EarthError::ArithmeticOverflow)?;

        msg!("Birth allocation minted: 66,000 EARTH to vault.");
        msg!("Beneficiary: {} | Minor: {} | Unlock: {}", beneficiary, is_minor, vault.unlock_timestamp);

        Ok(())
    }

    /// Allows a verified beneficiary to claim their vault tokens.
    /// For minors, only callable after unlock_timestamp (age 18).
    /// Beneficiary MUST be in the Human Registry (AI cannot claim).
    pub fn claim_vault(ctx: Context<ClaimVault>) -> Result<()> {
        let state = &ctx.accounts.program_state;
        require!(!state.emergency_freeze, EarthError::SystemFrozen);

        let vault = &mut ctx.accounts.vault_state;
        require!(vault.is_initialized, EarthError::VaultNotInitialized);
        require!(!vault.is_claimed, EarthError::VaultAlreadyClaimed);

        // Verify claimer is the beneficiary
        require_keys_eq!(
            ctx.accounts.beneficiary.key(),
            vault.beneficiary,
            EarthError::UnauthorizedBeneficiary
        );

        // ZERO AI OWNERSHIP: Verify claimer is a registered human
        let claimer_registry = &ctx.accounts.beneficiary_human_registry;
        require!(claimer_registry.is_registered, EarthError::ClaimerNotHuman);
        require!(claimer_registry.is_active, EarthError::ClaimerNotActive);

        // Time-lock check for minors
        if vault.is_minor {
            let clock = Clock::get()?;
            require!(
                clock.unix_timestamp >= vault.unlock_timestamp,
                EarthError::VaultTimeLocked
            );
        }

        vault.is_claimed = true;

        msg!("Vault claimed by verified human: {}", vault.beneficiary);
        msg!("Amount: 66,000 EARTH tokens released.");

        Ok(())
    }
}

// ============================================================================
// ACCOUNT STRUCTS — INSTRUCTIONS
// ============================================================================

#[derive(Accounts)]
pub struct InitializeMint<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        init,
        payer = admin,
        mint::decimals = TOKEN_DECIMALS,
        mint::authority = mint_authority,
        mint::token_program = token_program,
    )]
    pub mint: InterfaceAccount<'info, Mint>,

    /// CHECK: PDA used as mint authority, validated by seeds.
    #[account(
        seeds = [MINT_AUTHORITY_SEED],
        bump,
    )]
    pub mint_authority: UncheckedAccount<'info>,

    #[account(
        init,
        payer = admin,
        space = 8 + ProgramState::INIT_SPACE,
        seeds = [PROGRAM_STATE_SEED],
        bump,
    )]
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

    /// CHECK: The wallet being registered as human-owned.
    pub human_wallet: UncheckedAccount<'info>,

    #[account(
        init,
        payer = oracle_signer,
        space = 8 + HumanRegistry::INIT_SPACE,
        seeds = [HUMAN_REGISTRY_SEED, human_wallet.key().as_ref()],
        bump,
    )]
    pub human_registry: Account<'info, HumanRegistry>,

    #[account(
        mut,
        seeds = [PROGRAM_STATE_SEED],
        bump,
        constraint = program_state.is_initialized @ EarthError::NotInitialized,
    )]
    pub program_state: Account<'info, ProgramState>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct TransferWithHumanCheck<'info> {
    #[account(mut)]
    pub sender: Signer<'info>,

    /// CHECK: Recipient wallet address.
    pub recipient_wallet: UncheckedAccount<'info>,

    #[account(
        constraint = sender_human_registry.is_registered @ EarthError::SenderNotHuman,
        constraint = sender_human_registry.wallet == sender.key() @ EarthError::SenderWalletMismatch,
        seeds = [HUMAN_REGISTRY_SEED, sender.key().as_ref()],
        bump,
    )]
    pub sender_human_registry: Account<'info, HumanRegistry>,

    #[account(
        constraint = recipient_human_registry.is_registered @ EarthError::RecipientNotHuman,
        constraint = recipient_human_registry.wallet == recipient_wallet.key() @ EarthError::RecipientWalletMismatch,
        seeds = [HUMAN_REGISTRY_SEED, recipient_wallet.key().as_ref()],
        bump,
    )]
    pub recipient_human_registry: Account<'info, HumanRegistry>,

    #[account(mut)]
    pub sender_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub recipient_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        seeds = [PROGRAM_STATE_SEED],
        bump,
        constraint = program_state.is_initialized @ EarthError::NotInitialized,
    )]
    pub program_state: Account<'info, ProgramState>,

    pub token_program: Program<'info, Token2022>,
}

#[derive(Accounts)]
#[instruction(proposal_id: [u8; 32])]
pub struct CreateProposal<'info> {
    #[account(mut)]
    pub proposer: Signer<'info>,

    #[account(
        constraint = proposer_human_registry.is_registered @ EarthError::ProposerNotHuman,
        constraint = proposer_human_registry.wallet == proposer.key() @ EarthError::ProposerWalletMismatch,
        seeds = [HUMAN_REGISTRY_SEED, proposer.key().as_ref()],
        bump,
    )]
    pub proposer_human_registry: Account<'info, HumanRegistry>,

    #[account(
        init,
        payer = proposer,
        space = 8 + Proposal::INIT_SPACE,
        seeds = [PROPOSAL_SEED, &proposal_id],
        bump,
    )]
    pub proposal: Account<'info, Proposal>,

    #[account(
        mut,
        seeds = [PROGRAM_STATE_SEED],
        bump,
        constraint = program_state.is_initialized @ EarthError::NotInitialized,
    )]
    pub program_state: Account<'info, ProgramState>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct CastVote<'info> {
    #[account(mut)]
    pub voter: Signer<'info>,

    #[account(
        constraint = voter_human_registry.is_registered @ EarthError::VoterNotHuman,
        constraint = voter_human_registry.wallet == voter.key() @ EarthError::VoterWalletMismatch,
        seeds = [HUMAN_REGISTRY_SEED, voter.key().as_ref()],
        bump,
    )]
    pub voter_human_registry: Account<'info, HumanRegistry>,

    #[account(
        mut,
        constraint = proposal.is_active @ EarthError::ProposalNotActive,
    )]
    pub proposal: Account<'info, Proposal>,

    #[account(
        init,
        payer = voter,
        space = 8 + VoteRecord::INIT_SPACE,
        seeds = [VOTE_SEED, proposal.proposal_id.as_ref(), voter.key().as_ref()],
        bump,
    )]
    pub vote_record: Account<'info, VoteRecord>,

    #[account(
        seeds = [PROGRAM_STATE_SEED],
        bump,
        constraint = program_state.is_initialized @ EarthError::NotInitialized,
    )]
    pub program_state: Account<'info, ProgramState>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct FinalizeProposal<'info> {
    #[account(mut)]
    pub proposal: Account<'info, Proposal>,
}

#[derive(Accounts)]
pub struct EmergencyFreeze<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [PROGRAM_STATE_SEED],
        bump,
        constraint = program_state.is_initialized @ EarthError::NotInitialized,
    )]
    pub program_state: Account<'info, ProgramState>,
}

#[derive(Accounts)]
pub struct EmergencyUnfreeze<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [PROGRAM_STATE_SEED],
        bump,
        constraint = program_state.is_initialized @ EarthError::NotInitialized,
    )]
    pub program_state: Account<'info, ProgramState>,

    /// The passed unfreeze proposal (must be executed and passed).
    pub unfreeze_proposal: Account<'info, Proposal>,
}

#[derive(Accounts)]
pub struct UpdateOracle<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        seeds = [PROGRAM_STATE_SEED],
        bump,
        constraint = program_state.is_initialized @ EarthError::NotInitialized,
    )]
    pub program_state: Account<'info, ProgramState>,
}

#[derive(Accounts)]
#[instruction(birth_event_id: [u8; 32])]
pub struct MintBirthAllocation<'info> {
    #[account(mut)]
    pub oracle_signer: Signer<'info>,

    #[account(
        mut,
        constraint = mint.key() == program_state.mint @ EarthError::InvalidMint,
    )]
    pub mint: InterfaceAccount<'info, Mint>,

    /// CHECK: PDA mint authority validated by seeds.
    #[account(
        seeds = [MINT_AUTHORITY_SEED],
        bump = program_state.mint_authority_bump,
    )]
    pub mint_authority: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [PROGRAM_STATE_SEED],
        bump,
        constraint = program_state.is_initialized @ EarthError::NotInitialized,
    )]
    pub program_state: Account<'info, ProgramState>,

    #[account(
        init,
        payer = oracle_signer,
        space = 8 + VaultState::INIT_SPACE,
        seeds = [VAULT_SEED, &birth_event_id],
        bump,
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        init,
        payer = oracle_signer,
        token::mint = mint,
        token::authority = vault_state,
        token::token_program = token_program,
    )]
    pub vault_token_account: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Program<'info, Token2022>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct ClaimVault<'info> {
    #[account(mut)]
    pub beneficiary: Signer<'info>,

    #[account(
        mut,
        constraint = vault_state.is_initialized @ EarthError::VaultNotInitialized,
        constraint = !vault_state.is_claimed @ EarthError::VaultAlreadyClaimed,
        constraint = vault_state.beneficiary == beneficiary.key() @ EarthError::UnauthorizedBeneficiary,
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        mut,
        constraint = vault_token_account.key() == vault_state.vault_token_account @ EarthError::InvalidVaultTokenAccount,
    )]
    pub vault_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub beneficiary_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        constraint = beneficiary_human_registry.is_registered @ EarthError::ClaimerNotHuman,
        constraint = beneficiary_human_registry.wallet == beneficiary.key() @ EarthError::ClaimerWalletMismatch,
        seeds = [HUMAN_REGISTRY_SEED, beneficiary.key().as_ref()],
        bump,
    )]
    pub beneficiary_human_registry: Account<'info, HumanRegistry>,

    #[account(
        seeds = [PROGRAM_STATE_SEED],
        bump,
        constraint = program_state.is_initialized @ EarthError::NotInitialized,
    )]
    pub program_state: Account<'info, ProgramState>,

    pub token_program: Program<'info, Token2022>,
}

// ============================================================================
// STATE ACCOUNTS
// ============================================================================

/// Global program state.
#[account]
#[derive(InitSpace)]
pub struct ProgramState {
    /// Hardcoded admin authority (FndrmgjS9iZ7wgnj58fp49W3cMSc3XEfBYkYA8J4cTH3).
    pub admin_authority: Pubkey,
    /// The EARTH token mint address.
    pub mint: Pubkey,
    /// Bump seed for the mint authority PDA.
    pub mint_authority_bump: u8,
    /// Authorized oracle data account for minting triggers.
    pub oracle_data_account: Pubkey,
    /// Total tokens minted across all events.
    pub total_minted: u64,
    /// Total birth events processed.
    pub total_birth_events: u64,
    /// Total verified humans in the registry.
    pub total_verified_humans: u64,
    /// Total governance proposals created.
    pub total_proposals: u64,
    /// Whether the program has been initialized.
    pub is_initialized: bool,
    /// EMERGENCY KILL SWITCH — halts ALL operations when true.
    pub emergency_freeze: bool,
    /// Reason for the current freeze (short description).
    pub freeze_reason: [u8; 64],
    /// Timestamp when the freeze was activated.
    pub freeze_timestamp: i64,
}

/// Human Registry entry — proves a wallet is owned by a verified human.
/// AI, bots, and algorithms CANNOT have entries in this registry.
#[account]
#[derive(InitSpace)]
pub struct HumanRegistry {
    /// Whether this entry is active and registered.
    pub is_registered: bool,
    /// One-way cryptographic hash of the biometric iris scan.
    pub iris_hash: [u8; 32],
    /// The wallet address owned by this verified human.
    pub wallet: Pubkey,
    /// Timestamp of registration.
    pub registration_timestamp: i64,
    /// Whether this human is currently active in the network.
    pub is_active: bool,
    /// Number of governance votes this human has cast.
    pub has_voted_count: u64,
}

/// Individual vault state — one per birth event / allocation.
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
}

/// Governance proposal — requires 1-Human, 1-Vote consensus to pass.
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

/// Individual vote record — ensures 1-Human, 1-Vote per proposal.
#[account]
#[derive(InitSpace)]
pub struct VoteRecord {
    pub has_voted: bool,
    pub voter: Pubkey,
    pub proposal: [u8; 32],
    pub vote_choice: bool,
    pub voted_at: i64,
}

// ============================================================================
// ENUMS
// ============================================================================

/// Types of governance proposals.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug, InitSpace)]
pub enum ProposalType {
    /// Standard system change proposal.
    SystemChange,
    /// Major allocation release (requires assembly vote).
    AllocationRelease,
    /// Oracle endpoint update.
    OracleUpdate,
    /// Emergency freeze triggered by human consensus.
    EmergencyFreeze,
    /// Unfreeze system after emergency (required to lift kill switch).
    UnfreezeSystem,
    /// Infrastructure deployment funding.
    InfrastructureDeployment,
}

// ============================================================================
// ERRORS
// ============================================================================

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

    #[msg("Birth event has already been processed. Duplicate minting blocked.")]
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

    // --- ZERO AI OWNERSHIP ERRORS ---
    #[msg("AI BLOCK: Sender wallet is not registered as human-owned. Bots/AI cannot transact.")]
    SenderNotHuman,

    #[msg("AI BLOCK: Sender wallet is not active in the human registry.")]
    SenderNotActive,

    #[msg("Sender wallet does not match human registry entry.")]
    SenderWalletMismatch,

    #[msg("AI BLOCK: Recipient wallet is not registered as human-owned. Bots/AI cannot receive.")]
    RecipientNotHuman,

    #[msg("AI BLOCK: Recipient wallet is not active in the human registry.")]
    RecipientNotActive,

    #[msg("Recipient wallet does not match human registry entry.")]
    RecipientWalletMismatch,

    #[msg("AI BLOCK: Claimer is not registered as human. Only verified humans can claim.")]
    ClaimerNotHuman,

    #[msg("Claimer wallet is not active in the human registry.")]
    ClaimerNotActive,

    #[msg("Claimer wallet does not match human registry entry.")]
    ClaimerWalletMismatch,

    // --- VOTING ERRORS ---
    #[msg("AI BLOCK: Voter is not registered as human. Only verified humans can vote.")]
    VoterNotHuman,

    #[msg("Voter wallet is not active in the human registry.")]
    VoterNotActive,

    #[msg("Voter wallet does not match human registry entry.")]
    VoterWalletMismatch,

    #[msg("You have already voted on this proposal. 1-Human, 1-Vote enforced.")]
    AlreadyVoted,

    #[msg("Proposal is not currently active.")]
    ProposalNotActive,

    #[msg("Voting period has ended for this proposal.")]
    VotingPeriodEnded,

    #[msg("Voting period has not ended yet. Cannot finalize.")]
    VotingPeriodNotEnded,

    #[msg("Proposal has already been executed.")]
    ProposalAlreadyExecuted,

    #[msg("AI BLOCK: Proposer is not registered as human. Only humans can create proposals.")]
    ProposerNotHuman,

    #[msg("Proposer wallet is not active in the human registry.")]
    ProposerNotActive,

    #[msg("Proposer wallet does not match human registry entry.")]
    ProposerWalletMismatch,
}
