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
import { resolveNetworkConfig } from "./network-config";

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

const getTokenBalanceRaw = async (address: anchor.web3.PublicKey): Promise<bigint> => {
  const account = await getAccount(provider.connection, address);
  return account.amount;
};

const createMintManual = async (authority: anchor.web3.PublicKey): Promise<anchor.web3.Keypair> => {
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
    createInitializeMintInstruction(mintKp.publicKey, 6, authority, null, TOKEN_PROGRAM_ID)
  );

  await provider.sendAndConfirm(tx, [mintKp]);
  return mintKp;
};

const createTokenAccountManual = async (
  mint: anchor.web3.PublicKey,
  owner: anchor.web3.PublicKey,
  payer: anchor.web3.Keypair | null = null
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
    createInitializeAccountInstruction(tokenAccountKp.publicKey, mint, owner, TOKEN_PROGRAM_ID)
  );

  await provider.sendAndConfirm(tx, payer ? [tokenAccountKp, payer] : [tokenAccountKp]);
  return tokenAccountKp;
};

const mintToManual = async (
  mint: anchor.web3.PublicKey,
  destination: anchor.web3.PublicKey,
  authority: anchor.web3.PublicKey,
  amount: number
) => {
  const tx = new anchor.web3.Transaction().add(
    createMintToInstruction(mint, destination, authority, amount, [], TOKEN_PROGRAM_ID)
  );
  await provider.sendAndConfirm(tx, []);
};

const derivePixelPda = (x: number, y: number): anchor.web3.PublicKey => {
  const [pixelPda] = anchor.web3.PublicKey.findProgramAddressSync(
    [
      Buffer.from("pixel"),
      Buffer.from(Uint8Array.of(x & 0xff, (x >> 8) & 0xff)),
      Buffer.from(Uint8Array.of(y & 0xff, (y >> 8) & 0xff)),
    ],
    program.programId
  );
  return pixelPda;
};

const run = async () => {
  const buyerWallet = provider.wallet.publicKey;
  const [billboardPda] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("billboard")],
    program.programId
  );

  let blockMint: anchor.web3.PublicKey;
  let billboardState: any;

  try {
    billboardState = await program.account.billboardAccount.fetch(billboardPda);
    blockMint = billboardState.blockTokenMint;
  } catch {
    const blockMintKp = await createMintManual(buyerWallet);
    blockMint = blockMintKp.publicKey;

    await program.methods
      .initializeBillboard(runtimeConfig.walletInitialBuys, runtimeConfig.walletRebuyFees, blockMint)
      .accountsStrict({
        billboard: billboardPda,
        signer: buyerWallet,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    billboardState = await program.account.billboardAccount.fetch(billboardPda);
  }

  const usdcMint = (await createMintManual(buyerWallet)).publicKey;
  const seller = anchor.web3.Keypair.generate();

  const fundSellerTx = new anchor.web3.Transaction().add(
    anchor.web3.SystemProgram.transfer({
      fromPubkey: buyerWallet,
      toPubkey: seller.publicKey,
      lamports: 0.3 * anchor.web3.LAMPORTS_PER_SOL,
    })
  );
  await provider.sendAndConfirm(fundSellerTx, []);

  const buyerUsdc = (await createTokenAccountManual(usdcMint, buyerWallet)).publicKey;
  const sellerUsdc = (await createTokenAccountManual(usdcMint, seller.publicKey)).publicKey;
  const initialBuyDestination = (
    await createTokenAccountManual(usdcMint, runtimeConfig.walletInitialBuys)
  ).publicKey;
  const protocolUsdc = (await createTokenAccountManual(usdcMint, runtimeConfig.walletRebuyFees)).publicKey;
  const buyerBlock = (await createTokenAccountManual(blockMint, buyerWallet)).publicKey;

  await mintToManual(usdcMint, buyerUsdc, buyerWallet, 30 * MICRO_USDC);
  await mintToManual(usdcMint, sellerUsdc, buyerWallet, 10 * MICRO_USDC);
  await mintToManual(blockMint, buyerBlock, buyerWallet, 3000 * MICRO_BLOCK);

  const contentA = anchor.web3.Keypair.generate();
  await program.methods
    .createContent("Logo A", "Premier logo utilisateur", "https://example.com/logo-a")
    .accountsStrict({
      content: contentA.publicKey,
      authority: buyerWallet,
      systemProgram: anchor.web3.SystemProgram.programId,
    })
    .signers([contentA])
    .rpc();

  const coordsA: Array<[number, number]> = [
    [12, 42],
    [13, 42],
    [14, 42],
  ];

  for (const [x, y] of coordsA) {
    const pixelPda = derivePixelPda(x, y);
    await program.methods
      .buyPixel(x, y, 0xff00ffff, contentA.publicKey)
      .accountsStrict({
        billboard: billboardPda,
        pixel: pixelPda,
        signer: buyerWallet,
        buyerUsdc,
        usdcDestination: initialBuyDestination,
        usdcMint,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();
  }

  const contentB = anchor.web3.Keypair.generate();
  await program.methods
    .createContent("Logo B", "Deuxième logo même user", "https://example.com/logo-b")
    .accountsStrict({
      content: contentB.publicKey,
      authority: buyerWallet,
      systemProgram: anchor.web3.SystemProgram.programId,
    })
    .signers([contentB])
    .rpc();

  const coordsB: Array<[number, number]> = [
    [100, 101],
    [101, 101],
  ];

  for (const [x, y] of coordsB) {
    const pixelPda = derivePixelPda(x, y);
    await program.methods
      .buyPixel(x, y, 0x00ff00ff, contentB.publicKey)
      .accountsStrict({
        billboard: billboardPda,
        pixel: pixelPda,
        signer: buyerWallet,
        buyerUsdc,
        usdcDestination: initialBuyDestination,
        usdcMint,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();
  }

  const targetRebuy: [number, number] = [12, 42];
  const rebuyPda = derivePixelPda(targetRebuy[0], targetRebuy[1]);
  await program.methods
    .rebuyPixel(targetRebuy[0], targetRebuy[1], 0xff0000ff, null)
    .accountsStrict({
      billboard: billboardPda,
      pixel: rebuyPda,
      signer: seller.publicKey,
      buyerUsdc: sellerUsdc,
      sellerUsdc: buyerUsdc,
      protocolUsdc,
      usdcMint,
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .signers([seller])
    .rpc();

  const lockTarget: [number, number] = [13, 42];
  const lockPda = derivePixelPda(lockTarget[0], lockTarget[1]);
  const buyerBlockBeforeLock = await getTokenBalanceRaw(buyerBlock);

  await program.methods
    .lockPixel(lockTarget[0], lockTarget[1])
    .accountsStrict({
      owner: buyerWallet,
      billboard: billboardPda,
      pixel: lockPda,
      blockTokenMint: blockMint,
      ownerBlockToken: buyerBlock,
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .rpc();

  await program.methods
    .updatePixel(lockTarget[0], lockTarget[1], 0x0000ffff, contentB.publicKey)
    .accountsStrict({
      owner: buyerWallet,
      pixel: lockPda,
    })
    .rpc();

  await program.methods
    .updateContent("Logo B v2", "Mise à jour metadata", "https://example.com/logo-b-v2")
    .accountsStrict({
      content: contentB.publicKey,
      authority: buyerWallet,
    })
    .rpc();

  const reboughtPixel = await program.account.pixelAccount.fetch(rebuyPda);
  const lockedPixel = await program.account.pixelAccount.fetch(lockPda);
  const contentAState = await program.account.contentAccount.fetch(contentA.publicKey);
  const contentBState = await program.account.contentAccount.fetch(contentB.publicKey);
  const finalBillboard = await program.account.billboardAccount.fetch(billboardPda);

  if (reboughtPixel.owner.toBase58() !== seller.publicKey.toBase58()) {
    throw new Error("Rebuy failed: owner should be seller.");
  }
  if (toNum(reboughtPixel.currentPrice) !== 2 * MICRO_USDC) {
    throw new Error("Rebuy failed: price should be doubled.");
  }
  if (toNum(reboughtPixel.rebuyCount) !== 1) {
    throw new Error("Rebuy failed: rebuy_count should increment.");
  }
  if (lockedPixel.locked !== true) {
    throw new Error("Lock failed: pixel should be locked.");
  }
  if (toNum(lockedPixel.lockedAtSlot) <= 0) {
    throw new Error("Lock failed: locked_at_slot should be set.");
  }
  if (!lockedPixel.contentRef || lockedPixel.contentRef.toBase58() !== contentB.publicKey.toBase58()) {
    throw new Error("Update failed: locked pixel content_ref should point to content B.");
  }
  if (contentAState.name !== "Logo A") {
    throw new Error("Content A corrupted.");
  }
  if (contentBState.name !== "Logo B v2") {
    throw new Error("Content B update failed.");
  }

  const buyerBlockAfterLock = await getTokenBalanceRaw(buyerBlock);
  const burned = buyerBlockBeforeLock - buyerBlockAfterLock;
  if (burned !== BigInt(1000 * MICRO_BLOCK)) {
    throw new Error(`BLOCK burn mismatch: expected ${1000 * MICRO_BLOCK}, got ${burned.toString()}`);
  }

  if (toNum(finalBillboard.totalPixelsSold) < coordsA.length + coordsB.length) {
    throw new Error("Billboard total_pixels_sold should include all buys.");
  }
  if (toNum(finalBillboard.totalPixelsLocked) < 1) {
    throw new Error("Billboard total_pixels_locked should increment.");
  }

  console.log("Flow completed ✅");
  console.log("Billboard:", finalBillboard);
};

run().catch((err) => {
  console.error(err);
  throw err;
});
