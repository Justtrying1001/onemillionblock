import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import {
  TOKEN_PROGRAM_ID,
  createMint,
  getOrCreateAssociatedTokenAccount,
  mintTo,
} from "@solana/spl-token";

const provider = anchor.AnchorProvider.env();
anchor.setProvider(provider);

const program = anchor.workspace.OneMillionBlock as Program;

const run = async () => {
  const wallet = provider.wallet.publicKey;

  // 1) Créer un mint test qui joue le rôle d'un USDC fake
  const usdcMint = await createMint(
    provider.connection,
    // @ts-ignore
    provider.wallet.payer,
    wallet,
    null,
    6 // 6 decimals comme USDC
  );

  console.log("USDC mint:", usdcMint.toBase58());

  // 2) Créer ATA acheteur
  const buyerUsdc = await getOrCreateAssociatedTokenAccount(
    provider.connection,
    // @ts-ignore
    provider.wallet.payer,
    usdcMint,
    wallet
  );

  console.log("Buyer USDC ATA:", buyerUsdc.address.toBase58());

  // 3) Destination = même wallet pour le test
  const destinationUsdc = await getOrCreateAssociatedTokenAccount(
    provider.connection,
    // @ts-ignore
    provider.wallet.payer,
    usdcMint,
    wallet
  );

  console.log("Destination USDC ATA:", destinationUsdc.address.toBase58());

  // 4) Mint 10 USDC fake à l'acheteur
  await mintTo(
    provider.connection,
    // @ts-ignore
    provider.wallet.payer,
    usdcMint,
    buyerUsdc.address,
    wallet,
    10_000_000 // 10 USDC avec 6 décimales
  );

  console.log("Minted 10 fake USDC");

  const [billboardPda] = anchor.web3.PublicKey.findProgramAddressSync(
    [Buffer.from("billboard")],
    program.programId
  );

  // 5) Ré-initialiser billboard sur un nouveau déploiement si nécessaire
  // Si billboard existe déjà, cette partie peut échouer ; dans ce cas commente-la.
  try {
    const initTx = await program.methods
      .initializeBillboard(wallet, wallet, usdcMint)
      .accounts({
        billboard: billboardPda,
        signer: wallet,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .rpc();

    console.log("Initialize billboard tx:", initTx);
  } catch (e) {
    console.log("Billboard probablement déjà initialisé, on continue...");
  }

  // 6) Acheter un nouveau pixel
  const x = 44;
  const y = 18;

  const [pixelPda] = anchor.web3.PublicKey.findProgramAddressSync(
    [
      Buffer.from("pixel"),
      Buffer.from(Uint8Array.of(x & 0xff, (x >> 8) & 0xff)),
      Buffer.from(Uint8Array.of(y & 0xff, (y >> 8) & 0xff)),
    ],
    program.programId
  );

  const tx = await program.methods
  .buyPixel(
    x,
    y,
    "USDC test pixel",
    "Pixel acheté avec fake USDC",
    Buffer.from([9, 9, 9, 9]),
    "https://example.com/usdc"
  )
  .accountsStrict({
    billboard: billboardPda,
    pixel: pixelPda,
    signer: wallet,
    buyerUsdc: buyerUsdc.address,
    usdcDestination: destinationUsdc.address,
    usdcMint: usdcMint,
    tokenProgram: TOKEN_PROGRAM_ID,
    systemProgram: anchor.web3.SystemProgram.programId,
  })
  .rpc();

  console.log("Buy tx:", tx);
  console.log("Pixel PDA:", pixelPda.toBase58());

  const pixelAccount = await program.account.pixelAccount.fetch(pixelPda);
  console.log("Pixel account:", pixelAccount);

  const billboardAccount = await program.account.billboardAccount.fetch(
    billboardPda
  );
  console.log("Billboard account:", billboardAccount);
};

run().catch((err) => {
  console.error(err);
});