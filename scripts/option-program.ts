import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { OptionContract } from "../target/types/option_contract";
import {
  PublicKey,
  Keypair,
  SYSVAR_RENT_PUBKEY,
  LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import fs from "fs";
import os from "os";
import path from "path";
import { createKeyPairSignerFromBytes } from "@solana/kit";

import {
  createMint,
  mintTo,
  getOrCreateAssociatedTokenAccount,
  getAssociatedTokenAddressSync,
  TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
} from "@solana/spl-token";

let contractData;
let USDCMint = new PublicKey("3d79oe7AKxxHfLz11BXAnWqBX72rubLiQppUNoKGhMPk");
let WSOLMint = new PublicKey("349kUpx5gmhFhy3bmYFW6SqNteDyc4uUt4Do5nSRM5B7");
const WSOL_ORACLE = new PublicKey(
  "J83w4HKfqxwcq3BEMMkPFSppX3gqekLyLJBexebFVkix"
);
const USDC_ORACLE = new PublicKey(
  "5SSkXsEKQepHHAewytPVwdej4epN1nxgLVM84L4KXgy7"
);

const USDC_amount = 100_000_000_000_000;
const WSOL_amount = 100_000_000_000_000_000;
const walletPath = path.resolve(os.homedir(), ".config/solana/id.json");
const secret = JSON.parse(fs.readFileSync(walletPath, "utf8"));
const wallet = new anchor.Wallet(Keypair.fromSecretKey(new Uint8Array(secret)));

// const poolName = "SOL-USDC-LP-TEST-2";
let userWallet = Keypair.fromSecretKey(new Uint8Array(secret));

// Configure the client using the cluster from Anchor.toml
const provider = new anchor.AnchorProvider(
  new anchor.web3.Connection("https://api.devnet.solana.com"),
  wallet,
  anchor.AnchorProvider.defaultOptions()
);
anchor.setProvider(provider);
const program = anchor.workspace.OptionContract as Program<OptionContract>;

const [contract] = PublicKey.findProgramAddressSync(
  [Buffer.from("contract")],
  program.programId
);

const createMints = async () => {
  // Initial setup - mint tokens, set up accounts
  USDCMint = await createMint(
    provider.connection,
    wallet.payer,
    wallet.publicKey,
    wallet.publicKey,
    6, // Adjusted decimals to 6
    undefined,
    { commitment: "finalized" },
    TOKEN_PROGRAM_ID
  );
  console.log("USDC Mint:", USDCMint.toBase58());

  WSOLMint = await createMint(
    provider.connection,
    wallet.payer,
    wallet.publicKey,
    wallet.publicKey,
    9, // Adjusted decimals to 9
    undefined,
    { commitment: "finalized" },
    TOKEN_PROGRAM_ID
  );

  console.log("WSOL Mint:", WSOLMint.toBase58());
};

const MintTokens = async () => {
  // Create associated token account for user
  const userUSDCAccount = await getOrCreateAssociatedTokenAccount(
    provider.connection,
    wallet.payer,
    USDCMint,
    wallet.publicKey,
    true,
    "finalized",
    { commitment: "finalized" },
    TOKEN_PROGRAM_ID
  );

  // Mint tokens to user's token account for testing
  await mintTo(
    provider.connection,
    userWallet,
    USDCMint,
    userUSDCAccount.address,
    userWallet,
    USDC_amount
  );

  const userWSOLAccount = await getOrCreateAssociatedTokenAccount(
    provider.connection,
    wallet.payer,
    WSOLMint,
    wallet.publicKey,
    true,
    "finalized",
    { commitment: "finalized" },
    TOKEN_PROGRAM_ID
  );

  const userWSOLATA = userWSOLAccount.address;
  console.log("userWSOLATA:", userWSOLATA.toBase58());

  // Mint tokens to user's token account for testing
  await mintTo(
    provider.connection,
    userWallet,
    WSOLMint,
    userWSOLATA,
    userWallet,
    WSOL_amount
  );

  console.log("Minted tokens to user's token account");
};

const init = async () => {
  // Initialize the program

  console.log("Initializing program:", await program.programId.toBase58());

  const tx = await program.methods
    .initialize()
    .accounts({
      signer: wallet.publicKey,
    })
    .remainingAccounts([
      {
        isSigner: true,
        isWritable: true,
        pubkey: wallet.publicKey,
      },
    ])
    .signers([wallet.payer])
    .rpc();
  console.log("Program initialized", tx);
};

const addPool = async (_poolName: string) => {
  const [transferAuthorityPDA] = PublicKey.findProgramAddressSync(
    [Buffer.from("transfer_authority")],
    program.programId
  );
  const [multisigPDA] = PublicKey.findProgramAddressSync(
    [Buffer.from("multisig")],
    program.programId
  );
  const [contractPDA] = PublicKey.findProgramAddressSync(
    [Buffer.from("contract")],
    program.programId
  );
  const [poolPDA] = PublicKey.findProgramAddressSync(
    [Buffer.from("pool"), Buffer.from(_poolName)],
    program.programId
  );
  const [lp_token_mint] = PublicKey.findProgramAddressSync(
    [Buffer.from("lp_token_mint"), Buffer.from(_poolName)],
    program.programId
  );

  const addPoolTx = await program.methods
    .addPool({ name: _poolName })
    .accountsPartial({
      signer: wallet.publicKey,
      multisig: multisigPDA,
      contract: contractPDA,
      pool: poolPDA,
      lpTokenMint: lp_token_mint,
      transferAuthority: transferAuthorityPDA,
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .signers([wallet.payer])
    .rpc();

  console.log("addPoolTx: ", addPoolTx);
};

const removePool = async (_poolName: string) => {
  let newPool: PublicKey = contractData.pools[0];
  let poolData = await program.account.pool.fetch(newPool);
  console.log("poolData", poolData);

  const [multisigPDA] = PublicKey.findProgramAddressSync(
    [Buffer.from("multisig")],
    program.programId
  );
  const [contractPDA] = PublicKey.findProgramAddressSync(
    [Buffer.from("contract")],
    program.programId
  );
  const [poolPDA] = PublicKey.findProgramAddressSync(
    [Buffer.from("pool"), Buffer.from(_poolName)],
    program.programId
  );

  const removePoolTx = await program.methods
    .removePool({ poolName: _poolName })
    .accountsPartial({
      signer: wallet.publicKey,
      multisig: multisigPDA,
      contract: contractPDA,
      pool: poolPDA,
    })
    .signers([wallet.payer])
    .rpc();

  console.log("removePoolTx: ", removePoolTx);
};

const addCustodies = async (_poolName: string) => {
  let newPool: PublicKey = contractData.pools.pop();
  let poolData = await program.account.pool.fetch(newPool);
  console.log("poolData", poolData);

  const [multisigPDA] = PublicKey.findProgramAddressSync(
    [Buffer.from("multisig")],
    program.programId
  );
  const [contractPDA] = PublicKey.findProgramAddressSync(
    [Buffer.from("contract")],
    program.programId
  );
  const [poolPDA] = PublicKey.findProgramAddressSync(
    [Buffer.from("pool"), Buffer.from(_poolName)],
    program.programId
  );

  const [wsolCustody] = PublicKey.findProgramAddressSync(
    [Buffer.from("custody"), newPool.toBuffer(), WSOLMint.toBuffer()],
    program.programId
  );
  console.log("wsolCustody", wsolCustody.toBase58());
  const reallocPool_WSOL_Tx = await program.methods
    .reallocPool({
      ratios: [
        {
          target: new anchor.BN(60),
          min: new anchor.BN(40),
          max: new anchor.BN(70),
        },
      ],
      custodyKey: wsolCustody,
      poolName: poolData.name,
    })
    .accountsPartial({
      signer: wallet.publicKey,
      multisig: multisigPDA,
      contract: contractPDA,
      pool: poolPDA,
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .signers([wallet.payer])
    .rpc();
  console.log("reallocPool_WSOL_Tx: ", reallocPool_WSOL_Tx);

  const addCustody_WSOL_Tx = await program.methods
    .addCustody({
      oracle: WSOL_ORACLE,
      poolName: poolData.name,
    })
    .accounts({
      signer: wallet.publicKey,
      custodyTokenMint: WSOLMint,
    })
    .signers([wallet.payer])
    .rpc();
  console.log("addCustody_WSOL_Tx", addCustody_WSOL_Tx);

  const [usdcCustody] = PublicKey.findProgramAddressSync(
    [Buffer.from("custody"), newPool.toBuffer(), USDCMint.toBuffer()],
    program.programId
  );
  console.log("usdcCustody", usdcCustody.toBase58());

  const reallocPool_USDC_Tx = await program.methods
    .reallocPool({
      ratios: [
        {
          target: new anchor.BN(60),
          min: new anchor.BN(40),
          max: new anchor.BN(70),
        },
        {
          target: new anchor.BN(40),
          min: new anchor.BN(20),
          max: new anchor.BN(60),
        },
      ],
      custodyKey: usdcCustody,
      poolName: poolData.name,
    })
    .accountsPartial({
      signer: wallet.publicKey,
      multisig: multisigPDA,
      contract: contractPDA,
      pool: poolPDA,
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .signers([wallet.payer])
    .rpc();
  console.log("reallocPool_USDC_Tx: ", reallocPool_USDC_Tx);

  const addCustody_USDC_Tx = await program.methods
    .addCustody({
      oracle: USDC_ORACLE,
      poolName: poolData.name,
    })
    .accounts({
      signer: wallet.publicKey,
      custodyTokenMint: USDCMint,
    })
    .signers([wallet.payer])
    .rpc();
  console.log("addCustody_USDC_Tx", addCustody_USDC_Tx);
};

const addUSDCLiquidity = async (_poolName: string) => {
  let newPool: PublicKey = contractData.pools.pop();
  let poolData = await program.account.pool.fetch(newPool);
  console.log("poolData", poolData);

  let custodies = [];
  let oracles = [];
  for await (let custody of poolData.custodies) {
    let c = await program.account.custody.fetch(new PublicKey(custody));
    let ora = c.oracle;
    console.log("c.fees", c.fees);
    console.log("custody:", custody, "oracle: ", ora);
    custodies.push({ pubkey: custody, isSigner: false, isWritable: true });
    oracles.push({ pubkey: ora, isSigner: false, isWritable: true });
  }
  const remainingAccounts = custodies.concat(oracles);
  const fundingAccount = getAssociatedTokenAddressSync(
    USDCMint,
    wallet.publicKey
  );
  const [lpTokenMint] = PublicKey.findProgramAddressSync(
    [Buffer.from("lp_token_mint"), Buffer.from(_poolName)],
    program.programId
  );
  const [custodyTokenAccount] = PublicKey.findProgramAddressSync(
    [
      Buffer.from("custody_token_account"),
      newPool.toBuffer(),
      USDCMint.toBuffer(),
    ],
    program.programId
  );
  const lpTokenAccount = getAssociatedTokenAddressSync(
    lpTokenMint,
    wallet.publicKey
  );
  const [transferAuthority] = PublicKey.findProgramAddressSync(
    [Buffer.from("transfer_authority")],
    program.programId
  );

  const addLiquidity_USDC_Tx = await program.methods
    .addLiquidity({
      amountIn: new anchor.BN(1_000_000_000_000_000),
      minLpAmountOut: new anchor.BN(100_000),
      poolName: _poolName,
    })
    .accountsPartial({
      owner: wallet.publicKey,
      fundingAccount: fundingAccount,
      lpTokenAccount: lpTokenAccount,
      transferAuthority: transferAuthority,
      custodyMint: USDCMint,
      custodyOracleAccount: USDC_ORACLE,
    })
    .remainingAccounts(remainingAccounts)
    .signers([wallet.payer])
    .rpc();

  console.log("addLiquidity_USDC_Tx", addLiquidity_USDC_Tx);
};

const addWSOLLiquidity = async (_poolName: string) => {
  let newPool: PublicKey = contractData.pools.pop();
  let poolData = await program.account.pool.fetch(newPool);
  console.log("poolData:", poolData);

  let custodies = [];
  let oracles = [];
  for await (let custody of poolData.custodies) {
    let c = await program.account.custody.fetch(new PublicKey(custody));
    let ora = c.oracle;
    console.log("c.fees", c.fees);
    console.log("custody:", custody, "oracle: ", ora);
    custodies.push({ pubkey: custody, isSigner: false, isWritable: true });
    oracles.push({ pubkey: ora, isSigner: false, isWritable: true });
  }

  const remainingAccounts = custodies.concat(oracles);

  const fundingAccount = getAssociatedTokenAddressSync(
    WSOLMint,
    wallet.publicKey
  );
  const [lpTokenMint] = PublicKey.findProgramAddressSync(
    [Buffer.from("lp_token_mint"), Buffer.from(_poolName)],
    program.programId
  );
  const [custodyTokenAccount] = PublicKey.findProgramAddressSync(
    [
      Buffer.from("custody_token_account"),
      newPool.toBuffer(),
      WSOLMint.toBuffer(),
    ],
    program.programId
  );
  const lpTokenAccount = getAssociatedTokenAddressSync(
    lpTokenMint,
    wallet.publicKey
  );

  const addLiquidity_WSOL_Tx = await program.methods
    .addLiquidity({
      amountIn: new anchor.BN(100_000_000_000_000),
      minLpAmountOut: new anchor.BN(100_000),
      poolName: _poolName,
    })
    .accounts({
      owner: wallet.publicKey,
      fundingAccount: fundingAccount,
      custodyMint: WSOLMint,
      custodyOracleAccount: WSOL_ORACLE,
    })
    .remainingAccounts(remainingAccounts)
    .signers([wallet.payer])
    .rpc();
  console.log("addLiquidity_WSOL_Tx", addLiquidity_WSOL_Tx);
};

const openOption_Call = async (
  _poolName: string,
  _index: number,
  _amount: number,
  _strike: number,
  _period: number
) => {
  let newPool: PublicKey = contractData.pools.pop();
  let poolData = await program.account.pool.fetch(newPool);
  console.log("poolData", poolData);
  // Open Option Call
  const fundingAccount = getAssociatedTokenAddressSync(
    WSOLMint,
    wallet.publicKey
  );
  const [lpTokenMint] = PublicKey.findProgramAddressSync(
    [Buffer.from("lp_token_mint"), Buffer.from(_poolName)],
    program.programId
  );
  const [poolPDA] = PublicKey.findProgramAddressSync(
    [Buffer.from("pool"), Buffer.from(_poolName)],
    program.programId
  );
  const [custodyTokenAccount] = PublicKey.findProgramAddressSync(
    [
      Buffer.from("custody_token_account"),
      newPool.toBuffer(),
      WSOLMint.toBuffer(),
    ],
    program.programId
  );
  const lpTokenAccount = getAssociatedTokenAddressSync(
    lpTokenMint,
    wallet.publicKey
  );
  let wsolCustody: PublicKey;
  for await (let custody of poolData.custodies) {
    let c = await program.account.custody.fetch(new PublicKey(custody));
    let mint = c.mint;
    if (mint.toBase58() == WSOLMint?.toBase58()) {
      wsolCustody = custody;
    }
  }
  const [optionDetail] = PublicKey.findProgramAddressSync(
    [
      Buffer.from("option"),
      wallet.publicKey.toBuffer(),
      new anchor.BN(_index).toArrayLike(Buffer, "le", 8),
      newPool.toBuffer(),
      wsolCustody.toBuffer(),
    ],
    program.programId
  );
  console.log("optionDetail", optionDetail, "wsolCustody", wsolCustody);

  const tx = await program.methods
    .openOption({
      amount: new anchor.BN(_amount),
      strike: _strike,
      expiredTime: new anchor.BN(
        Math.floor(Date.now() / 1000) + 86400 * _period
      ),
      period: new anchor.BN(_period),
      poolName: _poolName,
    })
    .accountsPartial({
      owner: wallet.publicKey,
      fundingAccount: fundingAccount,
      custodyMint: WSOLMint,
      payCustodyMint: WSOLMint,
      custodyOracleAccount: WSOL_ORACLE,
      payCustodyOracleAccount: WSOL_ORACLE,
      lockedCustodyMint: WSOLMint,
      optionDetail: optionDetail,
      pool: poolPDA,
      custody: wsolCustody,
    })
    .signers([wallet.payer])
    .rpc(); // {skipPreflight: true}

  console.log("openOptionTx:", tx);
};

const closeOption_Call = async (_poolName: string, _index: number) => {
  let newPool: PublicKey = contractData.pools.pop();
  let poolData = await program.account.pool.fetch(newPool);
  console.log("poolData", newPool);
  // close Option Call

  const [poolPDA] = PublicKey.findProgramAddressSync(
    [Buffer.from("pool"), Buffer.from(_poolName)],
    program.programId
  );
  const fundingAccount = getAssociatedTokenAddressSync(
    WSOLMint,
    wallet.publicKey
  );
  const [lpTokenMint] = PublicKey.findProgramAddressSync(
    [Buffer.from("lp_token_mint"), Buffer.from(_poolName)],
    program.programId
  );
  const [custodyTokenAccount] = PublicKey.findProgramAddressSync(
    [
      Buffer.from("custody_token_account"),
      newPool.toBuffer(),
      WSOLMint.toBuffer(),
    ],
    program.programId
  );
  const [transferAuthority] = PublicKey.findProgramAddressSync(
    [Buffer.from("transfer_authority")],
    program.programId
  );
  const [user] = PublicKey.findProgramAddressSync(
    [Buffer.from("user"), wallet.publicKey.toBuffer()],
    program.programId
  );
  const [lockedCustody] = PublicKey.findProgramAddressSync(
    [Buffer.from("custody"), newPool.toBuffer(), WSOLMint.toBuffer()],
    program.programId
  );
  const [payCustody] = PublicKey.findProgramAddressSync(
    [Buffer.from("custody"), newPool.toBuffer(), WSOLMint.toBuffer()],
    program.programId
  );
  const [payCustodyTokenAccount] = PublicKey.findProgramAddressSync(
    [
      Buffer.from("custody_token_account"),
      newPool.toBuffer(),
      WSOLMint.toBuffer(),
    ],
    program.programId
  );
  const lpTokenAccount = getAssociatedTokenAddressSync(
    lpTokenMint,
    wallet.publicKey
  );
  let wsolCustody: PublicKey;
  for await (let custody of poolData.custodies) {
    let c = await program.account.custody.fetch(new PublicKey(custody));
    let mint = c.mint;
    if (mint.toBase58() == WSOLMint?.toBase58()) {
      wsolCustody = custody;
    }
  }
  const [optionDetail] = PublicKey.findProgramAddressSync(
    [
      Buffer.from("option"),
      wallet.publicKey.toBuffer(),
      new anchor.BN(_index).toArrayLike(Buffer, "le", 8),
      newPool.toBuffer(),
      wsolCustody.toBuffer(),
    ],
    program.programId
  );

  const tx = await program.methods
    .closeOption({
      optionIndex: new anchor.BN(_index),
      poolName: _poolName,
    })
    .accountsPartial({
      owner: wallet.publicKey,
      fundingAccount: fundingAccount,
      transferAuthority: transferAuthority,
      contract: contract,
      pool: poolPDA,
      user: user,
      custody: wsolCustody,
      payCustody: payCustody,
      lockedCustody: lockedCustody,
      payCustodyTokenAccount: payCustodyTokenAccount,
      optionDetail: optionDetail,
      custodyMint: WSOLMint,
      payCustodyMint: WSOLMint,
    })
    .signers([wallet.payer])
    .rpc(); // {skipPreflight: true}

  console.log("closeOptionTx:", tx);
};

const removeLiquidity = async (
  lp_amount: number,
  min_amount_out: number,
  _poolName: string,
  asset: PublicKey
) => {
  let newPool: PublicKey = contractData.pools.pop();
  let poolData = await program.account.pool.fetch(newPool);
  console.log("poolData", newPool);
  // close Option Call

  const [poolPDA] = PublicKey.findProgramAddressSync(
    [Buffer.from("pool"), Buffer.from(_poolName)],
    program.programId
  );
  const fundingAccount = getAssociatedTokenAddressSync(asset, wallet.publicKey);
  const [lpTokenMint] = PublicKey.findProgramAddressSync(
    [Buffer.from("lp_token_mint"), Buffer.from(_poolName)],
    program.programId
  );
  const receivingAccount = getAssociatedTokenAddressSync(
    asset,
    wallet.publicKey
  );
  const lpTokenAccount = getAssociatedTokenAddressSync(
    lpTokenMint,
    wallet.publicKey
  );
  console.log("lpTokenAccount", lpTokenAccount.toBase58());
  console.log("lpTokenMint", lpTokenMint.toBase58());
  const [custodyTokenAccount] = PublicKey.findProgramAddressSync(
    [
      Buffer.from("custody_token_account"),
      newPool.toBuffer(),
      asset.toBuffer(),
    ],
    program.programId
  );
  const [transferAuthority] = PublicKey.findProgramAddressSync(
    [Buffer.from("transfer_authority")],
    program.programId
  );
  let custodies = [];
  let oracles = [];
  for await (let custody of poolData.custodies) {
    let c = await program.account.custody.fetch(new PublicKey(custody));
    let ora = c.oracle;
    console.log("c.fees", c.fees);
    console.log("custody:", custody, "oracle: ", ora);
    custodies.push({ pubkey: custody, isSigner: false, isWritable: true });
    oracles.push({ pubkey: ora, isSigner: false, isWritable: true });
  }
  const remainingAccounts = custodies.concat(oracles);
  const [CustodyPDA] = PublicKey.findProgramAddressSync(
    [Buffer.from("custody"), newPool.toBuffer(), asset.toBuffer()],
    program.programId
  );

  const tx = await program.methods
    .removeLiquidity({
      lpAmountIn: new anchor.BN(lp_amount),
      minAmountOut: new anchor.BN(min_amount_out),
      poolName: _poolName,
    })
    .accountsPartial({
      owner: wallet.publicKey,
      receivingAccount: receivingAccount,
      transferAuthority: transferAuthority,
      contract: contract,
      pool: poolPDA,
      custody: CustodyPDA,
      custodyOracleAccount: asset == WSOLMint ? WSOL_ORACLE : USDC_ORACLE,
      custodyTokenAccount: custodyTokenAccount,
      lpTokenAccount: lpTokenAccount,
      lpTokenMint: lpTokenMint,
      custodyMint: asset,
      tokenProgram: TOKEN_PROGRAM_ID,
    })
    .remainingAccounts(remainingAccounts)
    .signers([wallet.payer])
    .rpc(); // {skipPreflight: true}

  console.log("removeLiquidityTx:", tx);
};

const main = async () => {
  // await MintTokens();
  // await init();

  contractData = await program.account.contract.fetch(contract);
  console.log("Wallet:", provider.wallet.publicKey.toBase58());

  // await addPool("SOL-USDC");
  // await addCustodies("SOL-USDC");
  // await addUSDCLiquidity("SOL-USDC");
  // await addWSOLLiquidity("SOL-USDC");
  await openOption_Call("SOL-USDC", 4, 10_000_000, 180, 7);
  // await closeOption_Call("SOL-USDC", 3);

  // await removeLiquidity(300000_000_000, 1, "SOL-USDC", WSOLMint);
};

main();
