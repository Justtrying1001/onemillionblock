import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import {
  TOKEN_PROGRAM_ID,
  MINT_SIZE,
  ACCOUNT_SIZE,
  createInitializeMintInstruction,
  createInitializeAccountInstruction,
  createMintToInstruction,
  getAccount,
  getMinimumBalanceForRentExemptMint,
  getMinimumBalanceForRentExemptAccount,
} from "@solana/spl-token";

const provider = anchor.AnchorProvider.env();
anchor.setProvider(provider);

const program = anchor.workspace.OneMillionBlock as Program;

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

const run = async () => {
  const buyerWallet = provider.wallet.publicKey;

  // 1) Mint fake USDC
  const usdcMintKp = await createMintManual(buyerWallet);
  const usdcMint = usdcMintKp.publicKey;
  console.log("USDC mint:", usdcMint.toBase58());

  // 2) Billboard PDA
  const [billboardPda] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("billboard")],
    program.programId
  );

  // 3) Récupération / init billboard + block mint conditionnel
  let blockMint: anchor.web3.PublicKey;
  let billboardState: any;
  try {
    billboardState = await program.account.billboardAccount.fetch(billboardPda);
    blockMint = billboardState.blockTokenMint;
    console.log(
      "Billboard déjà initialisé, réutilisation du block_token_mint existant"
    );
    console.log("Billboard PDA:", billboardPda.toBase58());
    console.log("BLOCK mint reused from billboard:", blockMint.toBase58());
  } catch (e) {
    const blockMintKp = await createMintManual(buyerWallet);
    blockMint = blockMintKp.publicKey;
    console.log("New BLOCK mint created:", blockMint.toBase58());

    const initTx = await program.methods
      .initializeBillboard(buyerWallet, buyerWallet, blockMint)
      .accountsStrict({
        billboard: billboardPda,
        signer: buyerWallet,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    console.log("Initialize billboard tx:", initTx);
    billboardState = await program.account.billboardAccount.fetch(billboardPda);
    console.log("Billboard initialized:", billboardPda.toBase58());
    console.log("BLOCK mint used for billboard:", blockMint.toBase58());
  }
  console.log("Final BLOCK mint for this run:", blockMint.toBase58());

  // 4) Seller wallet
  const seller = anchor.web3.Keypair.generate();
  await sendSolToSeller(seller, 0.2 * anchor.web3.LAMPORTS_PER_SOL);
  console.log("Seller wallet:", seller.publicKey.toBase58());

  // 5) Comptes token créés manuellement
  const buyerUsdcKp = await createTokenAccountManual(usdcMint, buyerWallet);
  const sellerUsdcKp = await createTokenAccountManual(usdcMint, seller.publicKey);
  const initialBuyDestinationKp = await createTokenAccountManual(usdcMint, buyerWallet);
  const protocolUsdcKp = await createTokenAccountManual(usdcMint, buyerWallet);
  const buyerBlockKp = await createTokenAccountManual(blockMint, buyerWallet);

  const buyerUsdc = buyerUsdcKp.publicKey;
  const sellerUsdc = sellerUsdcKp.publicKey;
  const initialBuyDestination = initialBuyDestinationKp.publicKey;
  const protocolUsdc = protocolUsdcKp.publicKey;
  const buyerBlock = buyerBlockKp.publicKey;

  console.log("Buyer USDC token account:", buyerUsdc.toBase58());
  console.log("Seller USDC token account:", sellerUsdc.toBase58());
  console.log(
    "Initial buy destination token account:",
    initialBuyDestination.toBase58()
  );
  console.log("Protocol USDC token account:", protocolUsdc.toBase58());
  console.log("Buyer BLOCK token account:", buyerBlock.toBase58());

  // 6) Mint fake USDC + BLOCK
  await mintToManual(usdcMint, buyerUsdc, buyerWallet, 10 * MICRO_USDC);
  await mintToManual(usdcMint, sellerUsdc, buyerWallet, 10 * MICRO_USDC);
  await mintToManual(blockMint, buyerBlock, buyerWallet, 2000 * MICRO_BLOCK);

  console.log("Minted fake tokens");

  await logTokenBalance("Buyer USDC before buy", buyerUsdc);
  await logTokenBalance("Seller USDC before buy", sellerUsdc);
  await logTokenBalance("Protocol USDC before rebuy", protocolUsdc);
  await logTokenBalance("Buyer BLOCK before lock", buyerBlock);

  // 7) Pixel fresh
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

  // 8) Achat initial par seller
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
      tokenProgram: TOKEN_PROGRAM_ID,
      systemProgram: anchor.web3.SystemProgram.programId,
    })
    .signers([seller])
    .rpc();

  console.log("Buy tx:", buyTx);

  const pixelAfterBuy = await program.account.pixelAccount.fetch(pixelPda);
  console.log("Pixel after buy:", pixelAfterBuy);

  await logTokenBalance("Buyer USDC after initial buy", buyerUsdc);
  await logTokenBalance("Seller USDC after initial buy", sellerUsdc);
  await logTokenBalance(
    "Initial buy destination after initial buy",
    initialBuyDestination
  );

  // 9) Rebuy par buyer
  const rebuyTx = await program.methods
    .rebuyPixel(
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
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .rpc();

  console.log("Rebuy tx:", rebuyTx);

  const pixelAfterRebuy = await program.account.pixelAccount.fetch(pixelPda);
  const billboardAfterRebuy = await program.account.billboardAccount.fetch(
    billboardPda
  );

  console.log("Pixel after rebuy:", pixelAfterRebuy);
  console.log("Billboard after rebuy:", billboardAfterRebuy);

  await logTokenBalance("Buyer USDC after rebuy", buyerUsdc);
  await logTokenBalance("Seller USDC after rebuy", sellerUsdc);
  await logTokenBalance("Protocol USDC after rebuy", protocolUsdc);

  if (pixelAfterRebuy.owner.toBase58() !== buyerWallet.toBase58()) {
    throw new Error("Rebuy failed: owner is not buyer wallet");
  }

  if (toNum(pixelAfterRebuy.currentPrice) !== 2 * MICRO_USDC) {
    throw new Error("Rebuy failed: currentPrice is not 2 USDC");
  }

  if (toNum(pixelAfterRebuy.rebuyCount) !== 1) {
    throw new Error("Rebuy failed: rebuyCount is not 1");
  }

  // 10) Lock pixel par owner final (buyer)
  const buyerBlockBeforeLock = await getTokenBalanceRaw(buyerBlock);

  const lockTx = await program.methods
    .lockPixel()
    .accountsStrict({
      owner: buyerWallet,
      billboard: billboardPda,
      pixel: pixelPda,
      blockTokenMint: blockMint,
      ownerBlockToken: buyerBlock,
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .rpc();

  console.log("Lock tx:", lockTx);

  const pixelAfterLock = await program.account.pixelAccount.fetch(pixelPda);
  const billboardAfterLock = await program.account.billboardAccount.fetch(
    billboardPda
  );
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

  if (toNum(billboardAfterLock.totalBlockBurned) !== 1000) {
    throw new Error("Lock failed: billboard.totalBlockBurned should be 1000");
  }

  const expectedBurnRaw = BigInt(1000 * MICRO_BLOCK);
  const actualBurnRaw = buyerBlockBeforeLock - buyerBlockAfterLock;
  if (actualBurnRaw !== expectedBurnRaw) {
    throw new Error(
      `Lock failed: expected BLOCK burn ${expectedBurnRaw.toString()}, got ${actualBurnRaw.toString()}`
    );
  }

  // 11) Update metadata après lock
  const newName = "Locked buyer pixel";
  const newDescription = "Metadata update after lock";
  const newImageData = Buffer.from([7, 7, 7, 7, 7]);
  const newUrl = "https://example.com/updated";

  const updateMetadataTx = await program.methods
    .updateMetadata(newName, newDescription, newImageData, newUrl)
    .accountsStrict({
      owner: buyerWallet,
      pixel: pixelPda,
    })
    .rpc();

  console.log("Update metadata tx:", updateMetadataTx);

  const pixelAfterUpdate = await program.account.pixelAccount.fetch(pixelPda);
  console.log("Pixel after update metadata:", pixelAfterUpdate);

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
