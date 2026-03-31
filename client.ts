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

const logTokenBalance = async (
  label: string,
  address: anchor.web3.PublicKey
) => {
  const account = await getAccount(provider.connection, address);
  console.log(`${label}:`, Number(account.amount));
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

  // 3) Init billboard si besoin
  try {
    const initTx = await program.methods
      .initializeBillboard(buyerWallet, buyerWallet, usdcMint)
      .accountsStrict({
        billboard: billboardPda,
        signer: buyerWallet,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    console.log("Initialize billboard tx:", initTx);
  } catch (e) {
    console.log("Billboard probablement déjà initialisé, on continue...");
  }

  // 4) Seller wallet
  const seller = anchor.web3.Keypair.generate();
  await sendSolToSeller(seller, 0.2 * anchor.web3.LAMPORTS_PER_SOL);
  console.log("Seller wallet:", seller.publicKey.toBase58());

  // 5) Comptes token créés manuellement
  const buyerUsdcKp = await createTokenAccountManual(usdcMint, buyerWallet);
  const sellerUsdcKp = await createTokenAccountManual(usdcMint, seller.publicKey);
  const initialBuyDestinationKp = await createTokenAccountManual(usdcMint, buyerWallet);
  const protocolUsdcKp = await createTokenAccountManual(usdcMint, buyerWallet);

  const buyerUsdc = buyerUsdcKp.publicKey;
  const sellerUsdc = sellerUsdcKp.publicKey;
  const initialBuyDestination = initialBuyDestinationKp.publicKey;
  const protocolUsdc = protocolUsdcKp.publicKey;

  console.log("Buyer token account:", buyerUsdc.toBase58());
  console.log("Seller token account:", sellerUsdc.toBase58());
  console.log(
    "Initial buy destination token account:",
    initialBuyDestination.toBase58()
  );
  console.log("Protocol token account:", protocolUsdc.toBase58());

  // 6) Mint fake USDC
  await mintToManual(usdcMint, buyerUsdc, buyerWallet, 10 * MICRO_USDC);
  await mintToManual(usdcMint, sellerUsdc, buyerWallet, 10 * MICRO_USDC);

  console.log("Minted 10 fake USDC to buyer and seller");

  await logTokenBalance("Buyer balance before buy", buyerUsdc);
  await logTokenBalance("Seller balance before buy", sellerUsdc);
  await logTokenBalance("Protocol balance before rebuy", protocolUsdc);

  // 7) Pixel fresh
  const x = 52;
  const y = 19;

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

  await logTokenBalance("Buyer balance after initial buy", buyerUsdc);
  await logTokenBalance("Seller balance after initial buy", sellerUsdc);
  await logTokenBalance(
    "Initial buy destination balance after initial buy",
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

  // 10) Etat final
  const pixelAfterRebuy = await program.account.pixelAccount.fetch(pixelPda);
  const billboardAfterRebuy = await program.account.billboardAccount.fetch(
    billboardPda
  );

  console.log("Pixel after rebuy:", pixelAfterRebuy);
  console.log("Billboard after rebuy:", billboardAfterRebuy);

  await logTokenBalance("Buyer balance after rebuy", buyerUsdc);
  await logTokenBalance("Seller balance after rebuy", sellerUsdc);
  await logTokenBalance("Protocol balance after rebuy", protocolUsdc);

  console.log("Expected after rebuy:");
  console.log("- pixel.owner = buyer wallet");
  console.log("- pixel.currentPrice = 2000000");
  console.log("- pixel.rebuyCount = 1");
  console.log("- seller received 1900000");
  console.log("- protocol received 100000");
};

run().catch((err) => {
  console.error(err);
});