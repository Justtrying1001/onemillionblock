import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import {
  TOKEN_PROGRAM_ID,
  MINT_SIZE,
  ACCOUNT_SIZE,
  createInitializeMintInstruction,
  createInitializeAccountInstruction,
  createInitializeMint2Instruction,
  createMintToInstruction,
  createApproveInstruction,
  getAccount,
  getAssociatedTokenAddress,
  createAssociatedTokenAccountInstruction,
  getMinimumBalanceForRentExemptMint,
  getMinimumBalanceForRentExemptAccount,
  ASSOCIATED_TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { resolveNetworkConfig } from "./network-config";

const MPL_TOKEN_METADATA_PROGRAM_ID = new anchor.web3.PublicKey(
  "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s"
);

const provider = anchor.AnchorProvider.env();
anchor.setProvider(provider);

const program = anchor.workspace.OneMillionBlock as Program;
const runtimeConfig = resolveNetworkConfig(process.env.ONE_MB_NETWORK);

const MICRO_USDC = 1_000_000;
const MICRO_BLOCK = 1_000_000;

const toNum = (value: any): number => {
  if (typeof value === "number") return value;
  if (typeof value === "bigint") return Number(value);
  if (value?.toNumber) return value.toNumber();
  if (value?.toString) return Number(value.toString());
  return Number(value);
};

const getTokenBalanceRaw = async (
  address: anchor.web3.PublicKey
): Promise<bigint> => {
  const account = await getAccount(provider.connection, address);
  return account.amount;
};

const logTokenBalance = async (
  label: string,
  address: anchor.web3.PublicKey
) => {
  const amount = await getTokenBalanceRaw(address);
  console.log(`${label}:`, amount.toString());
};

const sendSolToSeller = async (
  seller: anchor.web3.Keypair,
  lamports: number
) => {
  const tx = new anchor.web3.Transaction().add(
    anchor.web3.SystemProgram.transfer({
      fromPubkey: provider.wallet.publicKey,
      toPubkey: seller.publicKey,
      lamports,
    })
  );
  const sig = await provider.sendAndConfirm(tx, []);
  console.log("Fund seller tx:", sig);
};

// Crée un Mint SPL standard (6 décimales, authority = caller)
const createMintManual = async (
  authority: anchor.web3.PublicKey
): Promise<anchor.web3.Keypair> => {
  const mintKp = anchor.web3.Keypair.generate();
  const lamports = await getMinimumBalanceForRentExemptMint(provider.connection);

  const tx = new anchor.web3.Transaction().add(
    anchor.web3.SystemProgram.createAccount({
      fromPubkey: provider.wallet.publicKey,
      newAccountPubkey: mintKp.publicKey,
      lamports,
      space: MINT_SIZE,
      programId: TOKEN_PROGRAM_ID,
    }),
    createInitializeMintInstruction(
      mintKp.publicKey,
      6,
      authority,
      null,
      TOKEN_PROGRAM_ID
    )
  );

  const sig = await provider.sendAndConfirm(tx, [mintKp]);
  console.log("Create mint tx:", sig);
  return mintKp;
};

// Crée un Mint NFT (decimals=0, supply=1).
// freeze_authority = billboardPda (le programme est la freeze authority pour lock_pixel).
// mint_authority = signerPubkey (pour minter le token unique dans buy_pixel côté contrat).
const createNftMint = async (
  signerPubkey: anchor.web3.PublicKey,
  billboardPda: anchor.web3.PublicKey
): Promise<anchor.web3.Keypair> => {
  const mintKp = anchor.web3.Keypair.generate();
  const lamports = await getMinimumBalanceForRentExemptMint(provider.connection);

  const tx = new anchor.web3.Transaction().add(
    anchor.web3.SystemProgram.createAccount({
      fromPubkey: provider.wallet.publicKey,
      newAccountPubkey: mintKp.publicKey,
      lamports,
      space: MINT_SIZE,
      programId: TOKEN_PROGRAM_ID,
    }),
    // decimals=0, mint_authority=signer, freeze_authority=billboardPda
    createInitializeMintInstruction(
      mintKp.publicKey,
      0,
      signerPubkey,           // mint_authority : le signer pour pouvoir minter dans buy_pixel
      billboardPda,           // freeze_authority : le PDA programme pour lock_pixel (Phase 2C)
      TOKEN_PROGRAM_ID
    )
  );

  const sig = await provider.sendAndConfirm(tx, [mintKp]);
  console.log("Create NFT mint tx:", sig, "| mint:", mintKp.publicKey.toBase58());
  return mintKp;
};

const createTokenAccountManual = async (
  mint: anchor.web3.PublicKey,
  owner: anchor.web3.PublicKey
): Promise<anchor.web3.Keypair> => {
  const tokenAccountKp = anchor.web3.Keypair.generate();
  const lamports = await getMinimumBalanceForRentExemptAccount(provider.connection);

  const tx = new anchor.web3.Transaction().add(
    anchor.web3.SystemProgram.createAccount({
      fromPubkey: provider.wallet.publicKey,
      newAccountPubkey: tokenAccountKp.publicKey,
      lamports,
      space: ACCOUNT_SIZE,
      programId: TOKEN_PROGRAM_ID,
    }),
    createInitializeAccountInstruction(
      tokenAccountKp.publicKey,
      mint,
      owner,
      TOKEN_PROGRAM_ID
    )
  );

  const sig = await provider.sendAndConfirm(tx, [tokenAccountKp]);
  console.log("Create token account tx:", sig, "owner:", owner.toBase58());
  return tokenAccountKp;
};

const mintToManual = async (
  mint: anchor.web3.PublicKey,
  destination: anchor.web3.PublicKey,
  authority: anchor.web3.PublicKey,
  amount: number
) => {
  const tx = new anchor.web3.Transaction().add(
    createMintToInstruction(
      mint,
      destination,
      authority,
      amount,
      [],
      TOKEN_PROGRAM_ID
    )
  );
  const sig = await provider.sendAndConfirm(tx, []);
  console.log("MintTo tx:", sig);
};

// Dérive le PDA Metaplex Metadata pour un mint donné
const getMetadataPda = (mint: anchor.web3.PublicKey): anchor.web3.PublicKey => {
  const [metadataPda] = anchor.web3.PublicKey.findProgramAddressSync(
    [
      Buffer.from("metadata"),
      MPL_TOKEN_METADATA_PROGRAM_ID.toBuffer(),
      mint.toBuffer(),
    ],
    MPL_TOKEN_METADATA_PROGRAM_ID
  );
  return metadataPda;
};

// Crée ou récupère l'ATA d'un owner pour un mint donné
const getOrCreateAta = async (
  mint: anchor.web3.PublicKey,
  owner: anchor.web3.PublicKey,
  payer: anchor.web3.Keypair | null = null
): Promise<anchor.web3.PublicKey> => {
  const ata = await getAssociatedTokenAddress(mint, owner);
  try {
    await getAccount(provider.connection, ata);
    return ata;
  } catch {
    const tx = new anchor.web3.Transaction().add(
      createAssociatedTokenAccountInstruction(
        provider.wallet.publicKey,
        ata,
        owner,
        mint
      )
    );
    const sig = await provider.sendAndConfirm(tx, payer ? [payer] : []);
    console.log("Create ATA tx:", sig, "| ata:", ata.toBase58());
    return ata;
  }
};

const run = async () => {
  const buyerWallet = provider.wallet.publicKey;
  console.log("Runtime network:", runtimeConfig.network);
  console.log("Main wallets config:");
  console.log("  wallet_initial_buys:", runtimeConfig.walletInitialBuys.toBase58());
  console.log("  wallet_rebuy_fees  :", runtimeConfig.walletRebuyFees.toBase58());
  console.log("  bags project wallet:", runtimeConfig.bagsProjectWallet.toBase58(), "(hors contrat)");
  console.log("  deploy authority   :", runtimeConfig.deployAuthority.toBase58(), "(hors contrat)");

  if (runtimeConfig.network === "mainnet") {
    console.log(
      "Mode mainnet: ce script E2E (mints fake + rebuy + lock) est désactivé. Utiliser uniquement initialize_billboard avec la config mainnet."
    );
  }

  // 1) Billboard PDA
  const [billboardPda] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("billboard")],
    program.programId
  );

  // 2) Récupération / init billboard
  let blockMint: anchor.web3.PublicKey;
  let billboardState: any;
  try {
    billboardState = await program.account.billboardAccount.fetch(billboardPda);
    blockMint = billboardState.blockTokenMint;
    console.log("Billboard déjà initialisé, réutilisation du block_token_mint existant");
    console.log("BLOCK mint reused from billboard:", blockMint.toBase58());
    if (
      billboardState.walletInitialBuys.toBase58() !==
      runtimeConfig.walletInitialBuys.toBase58()
    ) {
      throw new Error(
        `Billboard wallet_initial_buys mismatch. on-chain=${billboardState.walletInitialBuys.toBase58()} config=${runtimeConfig.walletInitialBuys.toBase58()}`
      );
    }
    if (
      billboardState.walletRebuyFees.toBase58() !==
      runtimeConfig.walletRebuyFees.toBase58()
    ) {
      throw new Error(
        `Billboard wallet_rebuy_fees mismatch. on-chain=${billboardState.walletRebuyFees.toBase58()} config=${runtimeConfig.walletRebuyFees.toBase58()}`
      );
    }
  } catch (e: any) {
    if (
      typeof e?.message === "string" &&
      e.message.includes("Billboard wallet_")
    ) {
      throw e;
    }
    if (runtimeConfig.network === "mainnet") {
      if (!runtimeConfig.blockTokenMint) {
        throw new Error(
          "Mainnet config error: block_token_mint is TODO. Set it in network-config.ts before initialize_billboard."
        );
      }
      blockMint = runtimeConfig.blockTokenMint;
      console.log("Mainnet BLOCK mint from config:", blockMint.toBase58());
    } else {
      const blockMintKp = await createMintManual(buyerWallet);
      blockMint = blockMintKp.publicKey;
      console.log("Devnet BLOCK mint created:", blockMint.toBase58());
    }

    const initTx = await program.methods
      .initializeBillboard(
        runtimeConfig.walletInitialBuys,
        runtimeConfig.walletRebuyFees,
        blockMint
      )
      .accountsStrict({
        billboard: billboardPda,
        signer: buyerWallet,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    console.log("Initialize billboard tx:", initTx);
    billboardState = await program.account.billboardAccount.fetch(billboardPda);
    console.log("Billboard initialized:", billboardPda.toBase58());
  }
  console.log("Final BLOCK mint for this run:", blockMint.toBase58());

  if (runtimeConfig.network === "mainnet") {
    console.log("Mainnet initialize flow terminé.");
    return;
  }

  // 3) Mint fake USDC (devnet/local flow seulement)
  const usdcMintKp = await createMintManual(buyerWallet);
  const usdcMint = usdcMintKp.publicKey;
  console.log("USDC mint:", usdcMint.toBase58());

  // 4) Seller wallet
  const seller = anchor.web3.Keypair.generate();
  await sendSolToSeller(seller, 0.3 * anchor.web3.LAMPORTS_PER_SOL);
  console.log("Seller wallet:", seller.publicKey.toBase58());

  // 5) Comptes USDC
  const buyerUsdcKp = await createTokenAccountManual(usdcMint, buyerWallet);
  const sellerUsdcKp = await createTokenAccountManual(usdcMint, seller.publicKey);
  const initialBuyDestinationKp = await createTokenAccountManual(
    usdcMint,
    runtimeConfig.walletInitialBuys
  );
  const protocolUsdcKp = await createTokenAccountManual(
    usdcMint,
    runtimeConfig.walletRebuyFees
  );
  const buyerBlockKp = await createTokenAccountManual(blockMint, buyerWallet);

  const buyerUsdc = buyerUsdcKp.publicKey;
  const sellerUsdc = sellerUsdcKp.publicKey;
  const initialBuyDestination = initialBuyDestinationKp.publicKey;
  const protocolUsdc = protocolUsdcKp.publicKey;
  const buyerBlock = buyerBlockKp.publicKey;

  // 6) Mint fake USDC + BLOCK
  await mintToManual(usdcMint, buyerUsdc, buyerWallet, 10 * MICRO_USDC);
  await mintToManual(usdcMint, sellerUsdc, buyerWallet, 10 * MICRO_USDC);
  await mintToManual(blockMint, buyerBlock, buyerWallet, 2000 * MICRO_BLOCK);
  console.log("Minted fake tokens");

  await logTokenBalance("Seller USDC before buy", sellerUsdc);
  await logTokenBalance("Buyer BLOCK before lock", buyerBlock);

  // 7) Coordonnées pixel fresh
  const seed = Date.now();
  const x = seed % 1000;
  const y = Math.floor(seed / 1000) % 1000;
  console.log("Test coordinates:", { x, y });

  const [pixelPda] = anchor.web3.PublicKey.findProgramAddressSync(
    [
      Buffer.from("pixel"),
      Buffer.from(Uint8Array.of(x & 0xff, (x >> 8) & 0xff)),
      Buffer.from(Uint8Array.of(y & 0xff, (y >> 8) & 0xff)),
    ],
    program.programId
  );
  console.log("Pixel PDA:", pixelPda.toBase58());

  // 8) Créer le NFT mint pour ce pixel (Phase 2A)
  // mint_authority = seller (l'acheteur initial), freeze_authority = billboardPda (pour lock)
  const nftMintKp = await createNftMint(seller.publicKey, billboardPda);
  const nftMint = nftMintKp.publicKey;
  console.log("NFT mint:", nftMint.toBase58());

  // Metadata PDA Metaplex (dérivé par Metaplex depuis le NFT mint)
  const nftMetadataPda = getMetadataPda(nftMint);
  console.log("NFT metadata PDA:", nftMetadataPda.toBase58());

  // ATA du seller pour le NFT (sera créée par le contrat via init_if_needed)
  const sellerNftAta = await getAssociatedTokenAddress(nftMint, seller.publicKey);
  console.log("Seller NFT ATA:", sellerNftAta.toBase58());

  // ─────────────────────────────────────────────
  // 9) BUY PIXEL — achat initial par seller
  //    → Crée PixelAccount + NFT Metaplex + mint 1 NFT vers seller
  // ─────────────────────────────────────────────
  const buyTx = await program.methods
    .buyPixel(
      x,
      y,
      "Seller pixel",
      "Achat initial par seller",
      Buffer.from([1, 2, 3, 4]),
      "https://example.com/seller"
    )
    .accountsStrict({
      billboard: billboardPda,
      pixel: pixelPda,
      signer: seller.publicKey,
      buyerUsdc: sellerUsdc,
      usdcDestination: initialBuyDestination,
      usdcMint: usdcMint,
      nftMint: nftMint,
      nftMetadata: nftMetadataPda,
      buyerNftToken: sellerNftAta,
      tokenProgram: TOKEN_PROGRAM_ID,
      associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      metadataProgram: MPL_TOKEN_METADATA_PROGRAM_ID,
      systemProgram: anchor.web3.SystemProgram.programId,
      rent: anchor.web3.SYSVAR_RENT_PUBKEY,
    })
    .signers([seller])
    .rpc();

  console.log("Buy tx:", buyTx);

  const pixelAfterBuy = await program.account.pixelAccount.fetch(pixelPda);
  console.log("Pixel after buy:", pixelAfterBuy);
  console.log("NFT mint stored:", pixelAfterBuy.nftMint.toBase58());

  if (pixelAfterBuy.nftMint.toBase58() === anchor.web3.PublicKey.default.toBase58()) {
    throw new Error("Buy failed: nft_mint is still default pubkey");
  }

  await logTokenBalance("Seller USDC after buy", sellerUsdc);
  await logTokenBalance("Seller NFT balance after buy", sellerNftAta);

  // Vérifier que le seller possède bien 1 NFT
  const sellerNftBalance = await getTokenBalanceRaw(sellerNftAta);
  if (sellerNftBalance !== 1n) {
    throw new Error(`Buy failed: seller NFT balance should be 1, got ${sellerNftBalance}`);
  }

  // ─────────────────────────────────────────────
  // 10) REBUY PIXEL — rachat par buyer (Phase 2B)
  //     → Transfer USDC + Transfer NFT seller → buyer
  //     Prérequis : seller doit approve() le billboardPda comme delegate
  //     pour que le programme puisse transférer le NFT via CPI.
  // ─────────────────────────────────────────────

  // ATA du buyer pour le NFT
  const buyerNftAta = await getOrCreateAta(nftMint, buyerWallet);
  console.log("Buyer NFT ATA:", buyerNftAta.toBase58());

  // Le seller approve le billboardPda pour qu'il puisse transférer le NFT
  const approveTx = new anchor.web3.Transaction().add(
    createApproveInstruction(
      sellerNftAta,           // source token account
      billboardPda,           // delegate (le PDA programme)
      seller.publicKey,       // owner
      1,                      // amount
      [],
      TOKEN_PROGRAM_ID
    )
  );
  const approveSig = await provider.sendAndConfirm(approveTx, [seller]);
  console.log("NFT approve tx:", approveSig, "| delegate: billboard PDA");

  const rebuyTx = await program.methods
    .rebuyPixel(
      x,
      y,
      "Buyer rebuy pixel",
      "Racheté par buyer",
      Buffer.from([9, 9, 9, 9]),
      "https://example.com/rebuy"
    )
    .accountsStrict({
      billboard: billboardPda,
      pixel: pixelPda,
      signer: buyerWallet,
      buyerUsdc: buyerUsdc,
      sellerUsdc: sellerUsdc,
      protocolUsdc: protocolUsdc,
      usdcMint: usdcMint,
      nftMint: nftMint,
      sellerNftToken: sellerNftAta,
      buyerNftToken: buyerNftAta,
      tokenProgram: TOKEN_PROGRAM_ID,
      associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
      systemProgram: anchor.web3.SystemProgram.programId,
    })
    .rpc();

  console.log("Rebuy tx:", rebuyTx);

  const pixelAfterRebuy = await program.account.pixelAccount.fetch(pixelPda);
  const billboardAfterRebuy = await program.account.billboardAccount.fetch(billboardPda);
  console.log("Pixel after rebuy:", pixelAfterRebuy);

  if (pixelAfterRebuy.owner.toBase58() !== buyerWallet.toBase58()) {
    throw new Error("Rebuy failed: owner is not buyer wallet");
  }
  if (toNum(pixelAfterRebuy.currentPrice) !== 2 * MICRO_USDC) {
    throw new Error("Rebuy failed: currentPrice is not 2 USDC");
  }
  if (toNum(pixelAfterRebuy.rebuyCount) !== 1) {
    throw new Error("Rebuy failed: rebuyCount is not 1");
  }

  // Vérifier que le NFT est bien passé au buyer
  const buyerNftBalance = await getTokenBalanceRaw(buyerNftAta);
  const sellerNftBalanceAfterRebuy = await getTokenBalanceRaw(sellerNftAta);
  if (buyerNftBalance !== 1n) {
    throw new Error(`Rebuy failed: buyer NFT balance should be 1, got ${buyerNftBalance}`);
  }
  if (sellerNftBalanceAfterRebuy !== 0n) {
    throw new Error(`Rebuy failed: seller NFT balance should be 0, got ${sellerNftBalanceAfterRebuy}`);
  }
  console.log("NFT transfer verified ✓");

  await logTokenBalance("Buyer USDC after rebuy", buyerUsdc);
  await logTokenBalance("Seller USDC after rebuy", sellerUsdc);
  await logTokenBalance("Protocol USDC after rebuy", protocolUsdc);

  // ─────────────────────────────────────────────
  // 11) LOCK PIXEL — verrouillage par buyer (Phase 2C)
  //     → Burn 1 000 $BLOCK + freeze NFT token account du buyer
  //     Le NFT devient intransférable — irréversible
  // ─────────────────────────────────────────────
  const buyerBlockBeforeLock = await getTokenBalanceRaw(buyerBlock);

  const lockTx = await program.methods
    .lockPixel(x, y)
    .accountsStrict({
      owner: buyerWallet,
      billboard: billboardPda,
      pixel: pixelPda,
      blockTokenMint: blockMint,
      ownerBlockToken: buyerBlock,
      nftMint: nftMint,
      ownerNftToken: buyerNftAta,
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .rpc();

  console.log("Lock tx:", lockTx);

  const pixelAfterLock = await program.account.pixelAccount.fetch(pixelPda);
  const billboardAfterLock = await program.account.billboardAccount.fetch(billboardPda);
  const buyerBlockAfterLock = await getTokenBalanceRaw(buyerBlock);

  console.log("Pixel after lock:", pixelAfterLock);
  console.log("Billboard after lock:", billboardAfterLock);
  await logTokenBalance("Buyer BLOCK after lock", buyerBlock);

  if (pixelAfterLock.locked !== true) {
    throw new Error("Lock failed: pixel.locked should be true");
  }
  if (toNum(pixelAfterLock.lockedAtBlock) <= 0) {
    throw new Error("Lock failed: pixel.lockedAtBlock should be > 0");
  }
  if (toNum(billboardAfterLock.totalPixelsLocked) < 1) {
    throw new Error("Lock failed: billboard.totalPixelsLocked should be >= 1");
  }

  // Vérification delta (robuste aux runs multiples sur le même billboard)
  const expectedTotalBlockBurned =
    toNum(billboardAfterRebuy.totalBlockBurned) + (1000 * MICRO_BLOCK);
  if (toNum(billboardAfterLock.totalBlockBurned) !== expectedTotalBlockBurned) {
    throw new Error(
      `Lock failed: billboard.totalBlockBurned should be ${expectedTotalBlockBurned}, got ${toNum(billboardAfterLock.totalBlockBurned)}`
    );
  }

  // Vérification burn raw
  const expectedBurnRaw = BigInt(1000 * MICRO_BLOCK);
  const actualBurnRaw = buyerBlockBeforeLock - buyerBlockAfterLock;
  if (actualBurnRaw !== expectedBurnRaw) {
    throw new Error(
      `Lock failed: expected BLOCK burn ${expectedBurnRaw.toString()}, got ${actualBurnRaw.toString()}`
    );
  }

  // Vérifier que le compte NFT est bien gelé
  const frozenNftAccount = await getAccount(provider.connection, buyerNftAta);
  if (!frozenNftAccount.isFrozen) {
    throw new Error("Lock failed: NFT token account should be frozen");
  }
  console.log("NFT freeze verified ✓");

  // ─────────────────────────────────────────────
  // 12) UPDATE METADATA — après lock (toujours autorisé)
  // ─────────────────────────────────────────────
  const newName = "Locked buyer pixel";
  const newDescription = "Metadata update after lock";
  const newImageData = Buffer.from([7, 7, 7, 7, 7]);
  const newUrl = "https://example.com/updated";

  const updateMetadataTx = await program.methods
    .updateMetadata(x, y, newName, newDescription, newImageData, newUrl)
    .accountsStrict({
      owner: buyerWallet,
      pixel: pixelPda,
    })
    .rpc();

  console.log("Update metadata tx:", updateMetadataTx);

  const pixelAfterUpdate = await program.account.pixelAccount.fetch(pixelPda);

  if (pixelAfterUpdate.name !== newName) {
    throw new Error("Update metadata failed: name mismatch");
  }
  if (pixelAfterUpdate.description !== newDescription) {
    throw new Error("Update metadata failed: description mismatch");
  }
  if (pixelAfterUpdate.url !== newUrl) {
    throw new Error("Update metadata failed: url mismatch");
  }
  if (
    Buffer.from(pixelAfterUpdate.imageData).toString("hex") !==
    newImageData.toString("hex")
  ) {
    throw new Error("Update metadata failed: imageData mismatch");
  }
  if (pixelAfterUpdate.owner.toBase58() !== pixelAfterLock.owner.toBase58()) {
    throw new Error("Update metadata failed: owner changed");
  }
  if (pixelAfterUpdate.locked !== true) {
    throw new Error("Update metadata failed: locked should remain true");
  }
  if (toNum(pixelAfterUpdate.currentPrice) !== toNum(pixelAfterLock.currentPrice)) {
    throw new Error("Update metadata failed: currentPrice changed");
  }
  if (toNum(pixelAfterUpdate.rebuyCount) !== toNum(pixelAfterLock.rebuyCount)) {
    throw new Error("Update metadata failed: rebuyCount changed");
  }

  console.log("E2E script completed successfully ✅");
};

run().catch((err) => {
  console.error("Uncaught error:", err);
  throw err;
});
