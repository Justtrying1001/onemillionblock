# onemillionblock

Programme Anchor + script client de démonstration pour **The 1 Million Block** avec architecture:

- 1 pixel acheté = 1 `PixelAccount` on-chain léger
- metadata partagées via `ContentAccount`
- aucune dépendance NFT/Metaplex dans le flow pixel

## Build / validation

```bash
rustfmt --check lib.rs
anchor build
npm install
npx tsc --noEmit client.ts network-config.ts
```

## Exécution du flow client

Le script `client.ts` démontre ce scénario:

1. initialize billboard
2. create content A
3. buy plusieurs pixels avec `color + content_ref`
4. create content B
5. buy d'autres pixels avec `content_ref` différent
6. rebuy un pixel (95/5 + prix x2)
7. lock un pixel (burn 1000 $BLOCK)
8. update pixel même après lock
9. update content

## Convention sur `total_block_burned`

`BillboardAccount.total_block_burned` est stocké **en raw mint units on-chain**
(base units SPL token).  
Exemple: pour un mint à 6 décimales, burn de 1000 BLOCK = `1_000_000_000` unités raw.

Les clients/frontends doivent normaliser avec `10^decimals` pour afficher une valeur humaine.

```bash
ONE_MB_NETWORK=devnet node client.ts
```

## Déploiement devnet

```bash
anchor build
anchor deploy --provider.cluster devnet
```
