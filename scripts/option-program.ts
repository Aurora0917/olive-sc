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
  transfer,
  getOrCreateAssociatedTokenAccount,
  getAssociatedTokenAddressSync,
  TOKEN_PROGRAM_ID,
  TOKEN_2022_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
} from "@solana/spl-token";

import { createUmi } from '@metaplex-foundation/umi-bundle-defaults';
import {
  createSignerFromKeypair,
  signerIdentity,
  generateSigner,
  percentAmount
} from '@metaplex-foundation/umi';
import {
  createAndMint,
  TokenStandard,
  mplTokenMetadata
} from '@metaplex-foundation/mpl-token-metadata';

let contractData;
let USDCMint = new PublicKey("Fe7yM1wqx5ySZmSHJjNzkLuvBCU8BEnYpmxcpGwwBkZq");
let WSOLMint = new PublicKey("6fiDYq4uZgQQNUZVaBBcwu9jAUTWWBb7U8nmxt6BCaHY");
// let USDCMint = new PublicKey("2hPbZQe6G676DkWjZHpyoK1rzMtAwp8fFxAMsph2kCcV");
// let WSOLMint = new PublicKey("A2QZdZQKXDNk67Xwp3dpmE3nTXFQQhuULAL1usqtaM5d");
const WSOL_ORACLE = new PublicKey(
  "7UVimffxr9ow1uXYxsr4LHAcV58mLzhmwaeKvJ1pjLiE"
);
const USDC_ORACLE = new PublicKey(
  "Dpw1EAVrSB1ibxiDQyTAW6Zip3J4Btk2x4SgApQCeFbX"
);

const USDC_amount = 200_000_000_000_000;
const WSOL_amount = 200_000_000_000_000_000;
const walletPath = path.resolve(os.homedir(), ".config/solana/id.json");
const secret = JSON.parse(fs.readFileSync(walletPath, "utf8"));
const wallet = new anchor.Wallet(Keypair.fromSecretKey(new Uint8Array(secret)));

// const poolName = "SOL-USDC-LP-V-2";
let userWallet = Keypair.fromSecretKey(new Uint8Array(secret));

// Configure the client using the cluster from Anchor.toml
const provider = new anchor.AnchorProvider(
  new anchor.web3.Connection("https://api.devnet.solana.com"),
  // new anchor.web3.Connection("http://127.0.0.1:8899"),
  wallet,
  anchor.AnchorProvider.defaultOptions()
);
anchor.setProvider(provider);
const program = anchor.workspace.OptionContract as Program<OptionContract>;

const umi = createUmi(provider.connection.rpcEndpoint).use(mplTokenMetadata());

// Convert your wallet to Umi signer format
const umiKeypair = umi.eddsa.createKeypairFromSecretKey(wallet.payer.secretKey);
const umiSigner = createSignerFromKeypair(umi, umiKeypair);
umi.use(signerIdentity(umiSigner));

const [contract] = PublicKey.findProgramAddressSync(
  [Buffer.from("contract")],
  program.programId
);

const createMintsAlternative = async () => {
  console.log("🪙 Creating fungible tokens with Metaplex Umi...");

  try {
    // === CREATE USDC TOKEN ===
    console.log("💵 Creating USDC token...");

    const usdcMint = generateSigner(umi);

    await createAndMint(umi, {
      mint: usdcMint,
      authority: umi.identity,
      name: "Test USD Coin",
      symbol: "USDC",
      uri: "https://gateway.pinata.cloud/ipfs/bafkreibpfwakakaxznm2o6ekbmvsmdkzjv7riclnokwczmftjl6g4bdcn4",
      sellerFeeBasisPoints: percentAmount(0),
      decimals: 6,
      amount: 200_000_000_000_000, // 200M USDC with 6 decimals
      tokenOwner: umi.identity.publicKey,
      tokenStandard: TokenStandard.Fungible,
    }).sendAndConfirm(umi);

    USDCMint = new PublicKey(usdcMint.publicKey);
    console.log("✅ USDC Token created:", USDCMint.toBase58());

    // === CREATE WSOL TOKEN ===
    console.log("💎 Creating WSOL token...");

    const wsolMint = generateSigner(umi);

    await createAndMint(umi, {
      mint: wsolMint,
      authority: umi.identity,
      name: "Test Wrapped SOL",
      symbol: "WSOL",
      uri: "https://gateway.pinata.cloud/ipfs/bafkreiejkfbxqfenyaqfrw2wlbbtin7j4iyiuw3antgj2zmesdghefuwza",
      sellerFeeBasisPoints: percentAmount(0),
      decimals: 9,
      amount: 200_000_000_000_000_000, // 200M WSOL with 9 decimals
      tokenOwner: umi.identity.publicKey,
      tokenStandard: TokenStandard.Fungible,
    }).sendAndConfirm(umi);

    WSOLMint = new PublicKey(wsolMint.publicKey);
    console.log("✅ WSOL Token created:", WSOLMint.toBase58());

    console.log("🎉 All tokens created successfully with Umi!");

  } catch (error) {
    console.error("❌ Error creating tokens with Umi:", error);

    // Fallback to basic token creation
    console.log("🔄 Falling back to basic token creation...");

    USDCMint = await createMint(
      provider.connection,
      wallet.payer,
      wallet.publicKey,
      wallet.publicKey,
      6, // USDC decimals
      undefined,
      { commitment: "finalized" },
      TOKEN_PROGRAM_ID
    );
    console.log("📦 Fallback USDC Mint:", USDCMint.toBase58());

    WSOLMint = await createMint(
      provider.connection,
      wallet.payer,
      wallet.publicKey,
      wallet.publicKey,
      9, // WSOL decimals
      undefined,
      { commitment: "finalized" },
      TOKEN_PROGRAM_ID
    );
    console.log("📦 Fallback WSOL Mint:", WSOLMint.toBase58());
  }
};

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
  let newPool: PublicKey;

  for (const poolPubkey of contractData.pools) {
    try {
      // Fetch pool data to check its name
      const poolData = await program.account.pool.fetch(poolPubkey);

      // Assuming pool has a name field
      if (poolData.name === _poolName) {
        newPool = poolPubkey;
        break;
      }
    } catch (error) {
      console.log(`Error fetching pool ${poolPubkey.toBase58()}:`, error);
      continue;
    }
  }
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
      amountIn: new anchor.BN(1_000_000_000_000),
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
  const tokenBalance = await provider.connection.getTokenAccountBalance(lpTokenAccount);
  console.log("LP Token Balance:", tokenBalance.value.uiAmount);
  console.log("LP Token Mint Address:", lpTokenMint.toBase58());
};

const addWSOLLiquidity = async (_poolName: string) => {
  let newPool: PublicKey;

  for (const poolPubkey of contractData.pools) {
    try {
      // Fetch pool data to check its name
      const poolData = await program.account.pool.fetch(poolPubkey);

      // Assuming pool has a name field
      if (poolData.name === _poolName) {
        newPool = poolPubkey;
        break;
      }
    } catch (error) {
      console.log(`Error fetching pool ${poolPubkey.toBase58()}:`, error);
      continue;
    }
  }
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

const openOption_Call_USDC = async (
  _poolName: string,
  _index: number,
  _amount: number,
  _strike: number,
  _period: number
) => {
  let newPool: PublicKey;

  for (const poolPubkey of contractData.pools) {
    try {
      // Fetch pool data to check its name
      const poolData = await program.account.pool.fetch(poolPubkey);

      // Assuming pool has a name field
      if (poolData.name === _poolName) {
        newPool = poolPubkey;
        break;
      }
    } catch (error) {
      console.log(`Error fetching pool ${poolPubkey.toBase58()}:`, error);
      continue;
    }
  }
  let poolData = await program.account.pool.fetch(newPool);
  console.log("poolData", poolData);
  // Open Option Call
  const fundingAccount = getAssociatedTokenAddressSync(
    USDCMint,
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
      USDCMint.toBuffer(),
    ],
    program.programId
  );
  const lpTokenAccount = getAssociatedTokenAddressSync(
    lpTokenMint,
    wallet.publicKey
  );
  let usdcCustody: PublicKey;
  let wsolCustody: PublicKey;
  for await (let custody of poolData.custodies) {
    let c = await program.account.custody.fetch(new PublicKey(custody));
    let mint = c.mint;
    if (mint.toBase58() == USDCMint?.toBase58()) {
      usdcCustody = custody;
    } else {
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

  const usdcCustodyData = await program.account.custody.fetch(usdcCustody);


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
      payCustodyMint: USDCMint,
      custodyOracleAccount: WSOL_ORACLE,
      payCustodyOracleAccount: USDC_ORACLE,
      lockedCustodyMint: WSOLMint,
      optionDetail: optionDetail,
      pool: poolPDA,
      custody: wsolCustody,
      payCustody: usdcCustody,
    })
    .signers([wallet.payer])
    .rpc(); // {skipPreflight: true}

  console.log("openOptionTx:", tx);
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
        Math.floor(Date.now() / 1000) + /* 86400 */ 1 * _period
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

// const closeOption_Call = async (_poolName: string, _index: number) => {
//   let newPool: PublicKey;
//   for (const poolPubkey of contractData.pools) {
//     try {
//       const poolData = await program.account.pool.fetch(poolPubkey);
//       if (poolData.name === _poolName) {
//         newPool = poolPubkey;
//         break;
//       }
//     } catch (error) {
//       console.log(`Error fetching pool ${poolPubkey.toBase58()}:`, error);
//       continue;
//     }
//   }

//   if (!newPool) {
//     console.log("❌ Pool not found:", _poolName);
//     return;
//   }

//   const poolData = await program.account.pool.fetch(newPool);
//   console.log("poolData", poolData);
//   // close Option Call

//   const [poolPDA] = PublicKey.findProgramAddressSync(
//     [Buffer.from("pool"), Buffer.from(_poolName)],
//     program.programId
//   );
//   const fundingAccount = getAssociatedTokenAddressSync(
//     WSOLMint,
//     wallet.publicKey
//   );
//   const [lpTokenMint] = PublicKey.findProgramAddressSync(
//     [Buffer.from("lp_token_mint"), Buffer.from(_poolName)],
//     program.programId
//   );
//   const [custodyTokenAccount] = PublicKey.findProgramAddressSync(
//     [
//       Buffer.from("custody_token_account"),
//       newPool.toBuffer(),
//       WSOLMint.toBuffer(),
//     ],
//     program.programId
//   );
//   const [transferAuthority] = PublicKey.findProgramAddressSync(
//     [Buffer.from("transfer_authority")],
//     program.programId
//   );
//   const [user] = PublicKey.findProgramAddressSync(
//     [Buffer.from("user"), wallet.publicKey.toBuffer()],
//     program.programId
//   );
//   const [lockedCustody] = PublicKey.findProgramAddressSync(
//     [Buffer.from("custody"), newPool.toBuffer(), WSOLMint.toBuffer()],
//     program.programId
//   );
//   const [payCustody] = PublicKey.findProgramAddressSync(
//     [Buffer.from("custody"), newPool.toBuffer(), WSOLMint.toBuffer()],
//     program.programId
//   );
//   const [payCustodyTokenAccount] = PublicKey.findProgramAddressSync(
//     [
//       Buffer.from("custody_token_account"),
//       newPool.toBuffer(),
//       WSOLMint.toBuffer(),
//     ],
//     program.programId
//   );
//   const lpTokenAccount = getAssociatedTokenAddressSync(
//     lpTokenMint,
//     wallet.publicKey
//   );
//   let wsolCustody: PublicKey;
//   for await (let custody of poolData.custodies) {
//     let c = await program.account.custody.fetch(new PublicKey(custody));
//     let mint = c.mint;
//     if (mint.toBase58() == WSOLMint?.toBase58()) {
//       wsolCustody = custody;
//     }
//   }
  
//   const custodyData = await program.account.custody.fetch(wsolCustody);
//   const [optionDetail] = PublicKey.findProgramAddressSync(
//     [
//       Buffer.from("option"),
//       wallet.publicKey.toBuffer(),
//       new anchor.BN(_index).toArrayLike(Buffer, "le", 8),
//       newPool.toBuffer(),
//       wsolCustody.toBuffer(),
//     ],
//     program.programId
//   );

//   const tx = await program.methods
//     .closeOption({
//       optionIndex: new anchor.BN(_index),
//       poolName: _poolName,
//     })
//     .accountsPartial({
//       owner: wallet.publicKey,
//       fundingAccount: fundingAccount,
//       transferAuthority: transferAuthority,
//       contract: contract,
//       pool: poolPDA,
//       user: user,
//       custody: wsolCustody,
//       payCustody: payCustody,
//       lockedCustody: lockedCustody,
//       custodyOracleAccount: custodyData.oracle,
//       payCustodyOracleAccount: custodyData.oracle,
//       payCustodyTokenAccount: payCustodyTokenAccount,
//       optionDetail: optionDetail,
//       custodyMint: WSOLMint,
//       payCustodyMint: WSOLMint,
//     })
//     .signers([wallet.payer])
//     .rpc(); // {skipPreflight: true}

//   console.log("closeOptionTx:", tx);
// };

const exerciseOption_Call = async (_poolName: string, _index: number) => {
  // Find pool by name
  let newPool: PublicKey;
  for (const poolPubkey of contractData.pools) {
    try {
      const poolData = await program.account.pool.fetch(poolPubkey);
      if (poolData.name === _poolName) {
        newPool = poolPubkey;
        break;
      }
    } catch (error) {
      console.log(`Error fetching pool ${poolPubkey.toBase58()}:`, error);
      continue;
    }
  }

  if (!newPool) {
    console.log("❌ Pool not found:", _poolName);
    return;
  }

  const poolData = await program.account.pool.fetch(newPool);
  console.log("Pool found:", poolData.name);

  // Find WSOL custody from pool's custodies
  let wsolCustody: PublicKey;
  for (const custody of poolData.custodies) {
    const custodyData = await program.account.custody.fetch(new PublicKey(custody));
    if (custodyData.mint.equals(WSOLMint)) {
      wsolCustody = new PublicKey(custody);
      break;
    }
  }

  let usdcCustody: PublicKey;
  for (const custody of poolData.custodies) {
    const custodyData = await program.account.custody.fetch(new PublicKey(custody));
    if (custodyData.mint.equals(USDCMint)) {
      usdcCustody = new PublicKey(custody);
      break;
    }
  }

  if (!wsolCustody) {
    console.log("❌ WSOL custody not found in pool");
    return;
  }

  // Derive all required accounts
  const [poolPDA] = PublicKey.findProgramAddressSync(
    [Buffer.from("pool"), Buffer.from(_poolName)],
    program.programId
  );

  const fundingAccount = getAssociatedTokenAddressSync(USDCMint, wallet.publicKey);

  const [transferAuthority] = PublicKey.findProgramAddressSync(
    [Buffer.from("transfer_authority")],
    program.programId
  );

  const [user] = PublicKey.findProgramAddressSync(
    [Buffer.from("user"), wallet.publicKey.toBuffer()],
    program.programId
  );

  // ✅ CRITICAL: Derive locked custody PDA (not mint!)
  const [lockedCustody] = PublicKey.findProgramAddressSync(
    [Buffer.from("custody"), newPool.toBuffer(), USDCMint.toBuffer()],
    program.programId
  );

  const [lockedCustodyTokenAccount] = PublicKey.findProgramAddressSync(
    [Buffer.from("custody_token_account"), newPool.toBuffer(), USDCMint.toBuffer()],
    program.programId
  );

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

  console.log("🚀 Exercising option...");
  console.log("  Pool:", _poolName);
  console.log("  Index:", _index);
  console.log("  Funding Account:", fundingAccount.toBase58());
  console.log("  WSOL Custody:", wsolCustody.toBase58());
  console.log("  Locked Custody:", lockedCustody.toBase58());

  try {
    const tx = await program.methods
      .exerciseOption({
        optionIndex: new anchor.BN(_index),
        poolName: _poolName,
      })
      .accountsPartial({
        owner: wallet.publicKey,
        fundingAccount: fundingAccount,
        transferAuthority: transferAuthority,
        contract: contract,
        pool: poolPDA,

        // ✅ ACCOUNT ORDER MATCHES RUST STRUCT
        custodyMint: WSOLMint,              // Mint first
        lockedCustodyMint: USDCMint,        // Mint first
        custody: wsolCustody,               // Then dependent accounts
        user: user,
        optionDetail: optionDetail,
        lockedCustody: lockedCustody,       // ✅ FIXED: Use custody PDA, not mint!
        lockedCustodyTokenAccount: lockedCustodyTokenAccount,
        lockedOracle: USDC_ORACLE,
      })
      .signers([wallet.payer])
      .rpc();

    console.log("✅ Exercise option successful:", tx);
    return tx;

  } catch (error) {
    console.log("❌ Transaction failed:", error.message);
    if (error.logs) {
      console.log("Logs:", error.logs);
    }
    throw error;
  }
};

// ============================================================================
// 🎯 KEY FIXES MADE:
// ============================================================================

/*
❌ BEFORE (Your broken code):
.accounts({
  // ...
  lockedCustody: WSOLMint,           // ❌ WRONG! This is a mint, not custody PDA
  lockedCustodyMint: WSOLMint,       // ✅ Correct
  // ...
})

✅ AFTER (Fixed code):
.accounts({
  // ...
  lockedCustody: lockedCustody,      // ✅ CORRECT! This is the custody PDA
  lockedCustodyMint: WSOLMint,       // ✅ Correct
  // ...
})

🔍 EXPLANATION:
- lockedCustody expects: Box<Account<'info, Custody>> (custody PDA owned by your program)
- You were passing: WSOLMint (mint account owned by Token Program)
- Result: AccountOwnedByWrongProgram error

✅ SOLUTION:
- Pass the derived lockedCustody PDA instead of the mint
- Account order matches your fixed Rust struct
- All constraints should now validate correctly
*/

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

const createFundedTokenAccountsViaTransfer = async (targetWalletAddress: string) => {
  const targetWallet = new PublicKey(targetWalletAddress);
  console.log("🎯 Creating funded token accounts for:", targetWallet.toBase58());

  // Token amounts: 50M each
  const USDC_AMOUNT = 50_000_000_000_000;  // 50M USDC (6 decimals)
  const WSOL_AMOUNT = 50_000_000_000_000_000;  // 50M WSOL (9 decimals)

  try {
    // === GET YOUR EXISTING TOKEN ACCOUNTS ===
    const senderUSDCAccount = getAssociatedTokenAddressSync(
      USDCMint,
      wallet.publicKey
    );

    const senderWSOLAccount = getAssociatedTokenAddressSync(
      WSOLMint,
      wallet.publicKey
    );

    console.log("📤 Transferring from your existing accounts:");
    console.log("USDC source:", senderUSDCAccount.toBase58());
    console.log("WSOL source:", senderWSOLAccount.toBase58());

    // === CREATE TARGET USDC ACCOUNT ===
    console.log("💰 Creating target USDC token account...");

    const targetUSDCAccount = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      userWallet,            // Payer
      USDCMint,              // Token mint
      targetWallet,          // Owner (target wallet)
      true,                  // allowOwnerOffCurve
      "finalized",
      { commitment: "finalized" },
      TOKEN_PROGRAM_ID
    );

    console.log("✅ USDC account created:", targetUSDCAccount.address.toBase58());

    // === TRANSFER 50M USDC ===
    console.log("📤 Transferring 50M USDC...");

    const usdcTransferTx = await transfer(
      provider.connection,
      userWallet,                // Payer
      senderUSDCAccount,         // Source (your account)
      targetUSDCAccount.address, // Destination (target account)
      userWallet,                // Owner of source account
      USDC_AMOUNT,              // Amount to transfer
      [],                       // Additional signers
      { commitment: "finalized" },
      TOKEN_PROGRAM_ID
    );

    console.log("✅ USDC transfer complete, TX:", usdcTransferTx);

    // === CREATE TARGET WSOL ACCOUNT ===
    console.log("💰 Creating target WSOL token account...");

    const targetWSOLAccount = await getOrCreateAssociatedTokenAccount(
      provider.connection,
      userWallet,            // Payer
      WSOLMint,              // Token mint
      targetWallet,          // Owner (target wallet)
      true,                  // allowOwnerOffCurve
      "finalized",
      { commitment: "finalized" },
      TOKEN_PROGRAM_ID
    );

    console.log("✅ WSOL account created:", targetWSOLAccount.address.toBase58());

    // === TRANSFER 50M WSOL ===
    console.log("📤 Transferring 50M WSOL...");

    const wsolTransferTx = await transfer(
      provider.connection,
      userWallet,                // Payer
      senderWSOLAccount,         // Source (your account)
      targetWSOLAccount.address, // Destination (target account)
      userWallet,                // Owner of source account
      WSOL_AMOUNT,              // Amount to transfer
      [],                       // Additional signers
      { commitment: "finalized" },
      TOKEN_PROGRAM_ID
    );

    console.log("✅ WSOL transfer complete, TX:", wsolTransferTx);

    // === SUMMARY ===
    console.log("🚀 Successfully created funded accounts via transfer:");
    console.log(`📍 Target Wallet: ${targetWallet.toBase58()}`);
    console.log(`💵 USDC Account: ${targetUSDCAccount.address.toBase58()} (50M USDC)`);
    console.log(`💎 WSOL Account: ${targetWSOLAccount.address.toBase58()} (50M WSOL)`);

    return {
      targetWallet: targetWallet,
      usdcAccount: targetUSDCAccount.address,
      wsolAccount: targetWSOLAccount.address,
      usdcTransferTx: usdcTransferTx,
      wsolTransferTx: wsolTransferTx
    };

  } catch (error) {
    console.error("❌ Failed to create funded accounts:", error);
    throw error;
  }
};

const main = async () => {
  // await createMints();
  // await MintTokens();
  // await createMintsAlternative();
  // await init();

  contractData = await program.account.contract.fetch(contract);
  console.log("Wallet:", provider.wallet.publicKey.toBase58());

  // const targetAddress = "AmASwHejc5MNtnVRpA9wJVH5k8g4V297tbdBs8jKBaFG";
  // await createFundedTokenAccountsViaTransfer(targetAddress);

  // await addPool("SOL/USDC");
  // await removePool("SOLD-USDC");
  // await addCustodies("SOL/USDC");
  // await addUSDCLiquidity("SOL/USDC");
  await addWSOLLiquidity("SOL/USDC");
  // await openOption_Call("SOL/USDC", 21, 10_000_000_000, 150, 500);
  // await openOption_Call_USDC("SOL/USDC", 13, 1_000_000, 150, 7);
  // await closeOption_Call("SOL/USDC", 18);
  // await exerciseOption_Call("SOL/USDC", 20);

  // await removeLiquidity(300000_000_000, 1, "SOL-USDC", WSOLMint);
};

main();
