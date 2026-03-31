# onemillionblock

## Build / validation

Ce repo contient le programme Anchor (`lib.rs`) et un client script (`client.ts`).

### Prérequis minimaux

- Rust stable
- Solana CLI
- Anchor CLI (`anchor`)
- Node.js + npm

### Commandes

```bash
# Depuis /workspace/onemillionblock
rustfmt --check lib.rs
anchor build
npm install
npx tsc --noEmit client.ts
```

## Configuration réseau

La config est centralisée dans `network-config.ts`:

- `MAINNET_CONFIG`
- `DEVNET_CONFIG`

### Mainnet (déjà fixé)

- `walletInitialBuys`: `4S54Q3VJAhquMTyBkoGnyCVCawttDTspuWhhhvGT9tGi`
- `walletRebuyFees`: `2EjN1mGFepKG3sdgCgCzGc676L683DfVXC514doYACpu`
- `bagsProjectWallet`: `9JuFeQmH8Avbr9cgnquVh2tRXyzWqLj8kcszNykVxovq` *(hors contrat)*
- `deployAuthority`: `4zGjAP347PS2hBapuMCWWwQDhavFspx9MygBpACcDj8A` *(hors contrat)*
- `blockTokenMint`: `TODO` (à définir avant `initialize_billboard` mainnet)

### Variables d'environnement

- `ONE_MB_NETWORK=devnet` (par défaut)
- `ONE_MB_NETWORK=mainnet` pour le mode mainnet

## Exécution client

```bash
# Devnet / flow E2E complet
ONE_MB_NETWORK=devnet node client.ts

# Mainnet / initialize seulement
# (échoue explicitement tant que blockTokenMint est TODO)
ONE_MB_NETWORK=mainnet node client.ts
```

Le mode `mainnet` n'exécute pas le flow E2E (mints fake, rebuy/lock), il ne fait que la phase d'initialisation du billboard.
