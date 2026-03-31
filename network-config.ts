import * as anchor from "@coral-xyz/anchor";

export type NetworkName = "devnet" | "mainnet";

export type RuntimeNetworkConfig = {
  network: NetworkName;
  walletInitialBuys: anchor.web3.PublicKey;
  walletRebuyFees: anchor.web3.PublicKey;
  bagsProjectWallet: anchor.web3.PublicKey;
  deployAuthority: anchor.web3.PublicKey;
  /**
   * BLOCK mint can stay undefined until final mint is decided.
   * - devnet: usually generated/reused dynamically in test script
   * - mainnet: must be provided before initialize_billboard on-chain
   */
  blockTokenMint?: anchor.web3.PublicKey;
};

const pk = (value: string) => new anchor.web3.PublicKey(value);

export const MAINNET_CONFIG: RuntimeNetworkConfig = {
  network: "mainnet",
  walletInitialBuys: pk("4S54Q3VJAhquMTyBkoGnyCVCawttDTspuWhhhvGT9tGi"),
  walletRebuyFees: pk("2EjN1mGFepKG3sdgCgCzGc676L683DfVXC514doYACpu"),
  bagsProjectWallet: pk("9JuFeQmH8Avbr9cgnquVh2tRXyzWqLj8kcszNykVxovq"),
  deployAuthority: pk("4zGjAP347PS2hBapuMCWWwQDhavFspx9MygBpACcDj8A"),
  // TODO: set once final $BLOCK mint is known.
  blockTokenMint: undefined,
};

export const DEVNET_CONFIG: RuntimeNetworkConfig = {
  network: "devnet",
  // Dev/test defaults are intentionally separate from mainnet constants.
  walletInitialBuys: pk("4S54Q3VJAhquMTyBkoGnyCVCawttDTspuWhhhvGT9tGi"),
  walletRebuyFees: pk("2EjN1mGFepKG3sdgCgCzGc676L683DfVXC514doYACpu"),
  bagsProjectWallet: pk("9JuFeQmH8Avbr9cgnquVh2tRXyzWqLj8kcszNykVxovq"),
  deployAuthority: pk("4zGjAP347PS2hBapuMCWWwQDhavFspx9MygBpACcDj8A"),
  blockTokenMint: undefined,
};

export const resolveNetworkConfig = (
  value: string | undefined
): RuntimeNetworkConfig => {
  const network = (value ?? "devnet").toLowerCase();
  if (network === "mainnet" || network === "mainnet-beta") {
    return MAINNET_CONFIG;
  }
  return DEVNET_CONFIG;
};
