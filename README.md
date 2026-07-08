# EARTH Token — The Orbital Compact

**A Solana Token-2022 protocol anchoring token supply to verified real-world assets and human biometric identity.**

EARTH is a public utility protocol — open source, collectively governed, with no founder equity or centralized control. Governance resides entirely in a 1-Human, 1-Vote assembly.

---

## What Is This?

EARTH redefines how token supply is generated and controlled. Instead of arbitrary minting or inflationary schedules, tokens enter circulation only when real value is proven:

- A new human life is biometrically verified (triggering a 66,000 EARTH allocation to a time-locked vault)
- New physical infrastructure is registered and validated via decentralized oracles

No personal wallet holds mint authority. The mint is controlled exclusively by a Program Derived Address (PDA), ensuring supply can only change through programmatic oracle inputs.

---

## Core Protocol Rules

| Rule | Implementation |
|------|---------------|
| **PDA Mint Authority** | No wallet controls minting — only the program itself via `mint_authority` PDA |
| **Zero AI/Bot Ownership** | `transfer_with_human_check()` requires both parties in the on-chain Human Registry |
| **1-Human, 1-Vote Governance** | PDA-based vote records make double-voting impossible; 51% quorum required |
| **$100M Wealth Ceiling** | Hard cap per wallet/entity to prevent hoarding monopolies |
| **Emergency Kill Switch** | `emergency_freeze()` halts all operations; unfreeze requires admin + 72hr cooling + human consensus vote |
| **Minor Protection Vaults** | Child allocations time-locked for 18 years with zero parental access |
| **Dynamic Oracle Minting** | `mint_birth_allocation()` mints exactly 66,000 EARTH per verified birth event |

---

## Repository Structure

```
earth-token/
├── Anchor.toml                  # Anchor project configuration
├── Cargo.toml                   # Workspace configuration
├── programs/
│   └── earth/
│       ├── Cargo.toml           # Program dependencies
│       └── src/
│           └── lib.rs           # Core smart contract (all protocol logic)
├── docs/
│   ├── THE_ORBITAL_COMPACT.md   # Full constitutional framework
│   ├── ARCHITECTURE.md          # Technical architecture reference
│   └── CONTRIBUTING.md          # How to contribute
├── tests/                       # Integration tests (coming soon)
└── app/                         # Web dashboard (coming soon)
```

---

## Smart Contract Overview

The Anchor program (`programs/earth/src/lib.rs`) implements:

**Initialization**
- `initialize_mint` — Creates Token-2022 mint with PDA authority

**Human Identity**
- `register_human` — Oracle-verified biometric registration to on-chain Human Registry

**Token Operations**
- `mint_birth_allocation` — Oracle-triggered minting of 66,000 EARTH to vault PDA
- `transfer_with_human_check` — Protocol-level enforcement that both parties are verified humans
- `claim_vault` — Beneficiary claims their allocation (time-lock enforced for minors)

**Governance**
- `create_proposal` — Any verified human can submit proposals
- `cast_vote` — 1-Human, 1-Vote; PDA prevents double-voting
- `finalize_proposal` — Checks quorum (51%) and majority to pass

**Security**
- `emergency_freeze` — Instant halt of all contract operations
- `emergency_unfreeze` — Requires admin + 72hr wait + passed consensus vote
- `update_oracle` — Admin-only oracle endpoint configuration

---

## Admin Authority

The admin key (`FndrmgjS9iZ7wgnj58fp49W3cMSc3XEfBYkYA8J4cTH3`) is hardcoded into the program. It can:

- Trigger emergency freeze
- Update oracle endpoints
- Initiate the program

It **cannot**:

- Mint tokens
- Transfer tokens
- Override governance votes
- Unfreeze without human consensus

---

## Getting Started

### Prerequisites

- [Rust](https://rustup.rs/) (latest stable)
- [Solana CLI](https://docs.solana.com/cli/install-solana-cli-tools) (v1.18+)
- [Anchor CLI](https://www.anchor-lang.com/docs/installation) (v0.30+)

### Build

```bash
anchor build
```

### Generate Program Keypair

```bash
solana-keygen new -o target/deploy/earth-keypair.json --no-bip39-passphrase
```

### Deploy to Devnet

```bash
solana config set --url devnet
anchor deploy
```

---

## Looking for Contributors

We are actively seeking experienced developers to join the core team:

- **Solana Anchor (Rust)** — Production-grade program development
- **Oracle Integration** — Switchboard, Pyth, or custom oracle bridges for biometric/census data
- **Decentralized Identity** — World ID, iris-hash protocols, Wormhole State Bridge
- **Zero-Knowledge Proofs** — Privacy-preserving biometric verification
- **Frontend (React/TypeScript)** — Dashboard integration with wallet connection

See [CONTRIBUTING.md](docs/CONTRIBUTING.md) for details on how to get involved.

**Contact:** [YOUR_EMAIL@protonmail.com]

---

## License

This project is open source under the [MIT License](LICENSE).

---

*EARTH: THE ORBITAL COMPACT © 2026 — Open Source Public Utility | Governed by 1-Human, 1-Vote Consensus*
