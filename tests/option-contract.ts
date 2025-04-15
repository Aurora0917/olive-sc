import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { OptionContract } from "../target/types/option_contract";
import {
  PublicKey,
  Keypair,
  SYSVAR_RENT_PUBKEY,
  LAMPORTS_PER_SOL,
} from "@solana/web3.js";
import {
  TOKEN_PROGRAM_ID,
  createMint,
  mintTo,
  getOrCreateAssociatedTokenAccount,
  getAssociatedTokenAddressSync,
} from "@solana/spl-token";
import fs from "fs";
const secret = JSON.parse(fs.readFileSync(`./user.json`, "utf8"));

const airdropSol = async (
  connection: anchor.web3.Connection,
  publicKey: PublicKey,
  amount: number
) => {
  const airdropSignature = await connection.requestAirdrop(publicKey, amount);
  await connection.confirmTransaction(airdropSignature, "confirmed");
};

describe("option-contract", async () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const localWallet = anchor.Wallet.local().payer;
  const program = anchor.workspace.OptionContract as Program<OptionContract>;
  let usdcMint, ownerATA, userATA;
  let wsolMint, ownerWSOLATA, userWSOLATA;
  let userWallet = Keypair.fromSecretKey(new Uint8Array(secret));

  usdcMint = new PublicKey("4dfkxzRKJzwhWHAkJErU4YVKr8RVKESDFj5xKqGuw7Xs");
  wsolMint = new PublicKey("AvGyRAUiWkF6fzALe1LNnzCwGbNTZ4aqyfthuEZHM5Wq");
  const SOL_PYTH_FEED = new PublicKey(
    "J83w4HKfqxwcq3BEMMkPFSppX3gqekLyLJBexebFVkix"
  );
  let contractData, multisigData
  before(async () => {
    const [multisig] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("multisig"),
      ],
      program.programId
    );
    multisigData = await program.account.multisig.fetch(multisig);
    const [contract] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("contract"),
      ],
      program.programId
    );
    contractData = await program.account.contract.fetch(contract);
  });
  // before(async () => {
  //   //
  //   await airdropSol(
  //     provider.connection,
  //     userWallet.publicKey,
  //     5 * LAMPORTS_PER_SOL
  //   );
  //   // Initial setup - mint tokens, set up accounts
  //   // usdcMint = await createMint(
  //   //   provider.connection,
  //   //   localWallet,
  //   //   localWallet.publicKey,
  //   //   null,
  //   //   6, // Adjusted decimals to 6
  //   //   undefined,
  //   //   {},
  //   //   TOKEN_PROGRAM_ID
  //   // );

  //   // Create associated token account for owner
  //   const ownerTokenAccount = await getOrCreateAssociatedTokenAccount(
  //     provider.connection,
  //     localWallet,
  //     usdcMint,
  //     localWallet.publicKey
  //   );

  //   ownerATA = ownerTokenAccount.address;

  //   console.log("ownerTokenAccount:", ownerATA.toBase58());

  //   // Mint tokens to owner's token account for testing
  //   // await mintTo(
  //   //   provider.connection,
  //   //   localWallet,
  //   //   usdcMint,
  //   //   ownerATA,
  //   //   localWallet,
  //   //   1_000_000_000 // 1,000,000 tokens (assuming 6 decimals)
  //   // );
  //   // Create associated token account for user
  //   const userTokenAccount = await getOrCreateAssociatedTokenAccount(
  //     provider.connection,
  //     userWallet,
  //     usdcMint,
  //     userWallet.publicKey
  //   );

  //   userATA = userTokenAccount.address;
  //   console.log("userATA:", userATA.toBase58());

  //   // Mint tokens to user's token account for testing
  //   // await mintTo(
  //   //   provider.connection,
  //   //   localWallet,
  //   //   usdcMint,
  //   //   userATA,
  //   //   localWallet,
  //   //   1_000_000_000 // 1,000,000 tokens
  //   // );

  //   // Initial setup - mint tokens, set up accounts
  //   // wsolMint = await createMint(
  //   //   provider.connection,
  //   //   localWallet,
  //   //   localWallet.publicKey,
  //   //   null,
  //   //   6, // Adjusted decimals to 6
  //   //   undefined,
  //   //   {},
  //   //   TOKEN_PROGRAM_ID
  //   // );

  //   // Create associated token account for owner
  //   const ownerWSOLTokenAccount = await getOrCreateAssociatedTokenAccount(
  //     provider.connection,
  //     localWallet,
  //     wsolMint,
  //     localWallet.publicKey
  //   );

  //   ownerWSOLATA = ownerWSOLTokenAccount.address;

  //   console.log(
  //     "ownerWSOLTokenAccount:",
  //     ownerWSOLATA.toBase58()
  //   );

  //   // Mint tokens to owner's token account for testing
  //   // await mintTo(
  //   //   provider.connection,
  //   //   localWallet,
  //   //   wsolMint,
  //   //   ownerWSOLATA,
  //   //   localWallet,
  //   //   1_000_000_000 // 1,000,000 tokens (assuming 6 decimals)
  //   // );

  //   // Create associated token account for user
  //   const userWSOLTokenAccount = await getOrCreateAssociatedTokenAccount(
  //     provider.connection,
  //     userWallet,
  //     wsolMint,
  //     userWallet.publicKey
  //   );

  //   userWSOLATA = userWSOLTokenAccount.address;

  //   console.log("userwsolata:", userWSOLATA.toBase58());

  //   // Mint tokens to user's token account for testing
  //   // await mintTo(
  //   //   provider.connection,
  //   //   localWallet,
  //   //   wsolMint,
  //   //   userWSOLATA,
  //   //   localWallet,
  //   //   1_000_000_000 // 1,000,000 tokens
  //   // );
  // });

  // it("initialization", async () => {
  //   // Add your test here.
  //   console.log(localWallet.publicKey.toBase58());

  //   const tx = await program.methods
  //     .initialize()
  //     .accounts({
  //       signer: localWallet.publicKey,
  //     }).remainingAccounts([{
  //       isSigner: true,
  //       isWritable: true,
  //       pubkey: localWallet.publicKey
  //     }])
  //     .signers([localWallet])
  //     .rpc();
  //    // tx // UNBJ1C8FbfLas8JBZHWaoxtbMrbitR8a81xjxHWuH1L2v7ek1R6Rt4NXBPieNiy9vuHSpS2Tte6m24VPwio6A5J
  //   console.log("Your transaction signature", tx);
  // });


  // Removed
  // it("create_lp_mint", async () => {
  //   // Add your test here.
  //   console.log(userWallet.publicKey.toBase58());

  //   const tx = await program.methods
  //     .createLpMint({name:"SOL-USDC", symbol:"LP1", uri:""})
  //     .accounts({
  //       signer: localWallet.publicKey,
  //     })
  //     .signers([localWallet])
  //     .rpc();

  //   console.log("Your transaction signature", tx);
  // });

  // it("add_pool", async () => {
  //   // Add your test here.

  //   const tx = await program.methods
  //     .addPool({name:"SOL-USDC"})
  //     .accounts({
  //       signer: localWallet.publicKey,
  //     })
  //     .signers([localWallet])
  //     .rpc();

  //     // AEPAUndXWqmuCVes7zDdiDquo9smseoXBX6ZpKvj3zfTCui7J28FbGJGW1T4E7aFoCoqJZXgMoDPkk54V3muayQ
  //   console.log("Your transaction signature", tx);
  // });

  // it("add_custody_sol", async () => {
  // Add your test here.
  // let newPool: PublicKey = contractData.pools.pop();
  // let poolData = await program.account.pool.fetch(newPool);
  // const [custody] = PublicKey.findProgramAddressSync(
  //   [
  //     Buffer.from("custody"),
  //     newPool.toBuffer(),
  //     wsolMint.toBuffer(),
  //   ],
  //   program.programId
  // );
  // console.log("custody", custody)
  // const tx = await program.methods
  //   .reallocPool({ ratios: [{ target: new anchor.BN(60), min: new anchor.BN(40), max: new anchor.BN(70) }], custodyKey: custody, poolName: poolData.name })
  //   .accounts({
  //     signer: localWallet.publicKey,
  //   })
  //   .signers([localWallet])
  //   .rpc();

  // S5wxnnTWJ2KrPkQkXFVLafVu6NnqHPxgW9x2hZ3AodRU3xMsymEmfYd18D9sQDJ1xbwRtAygycnpZMhdehW5Bc7
  // console.log("Your transaction signature", tx);

  // const tx = await program.methods
  //   .addCustody({ oracle: new PublicKey("J83w4HKfqxwcq3BEMMkPFSppX3gqekLyLJBexebFVkix"), poolName: poolData.name })
  //   .accounts({
  //     signer: localWallet.publicKey,
  //     custodyTokenMint: wsolMint
  //   })
  //   .signers([localWallet])
  //   .rpc();

  // 2y9ZGrML2w2Cne77M51pNXYYJfvqCH4Kc3r75YEZWPm4JKPUQ6ZrgDi8dbUeoEt1hHjtDBrt2ir2R7EJMYx1Tigw
  // console.log("Your transaction signature", tx);
  // });

  // it("add_custody_usdc", async () => {
  //   // Add your test here.
  //   let newPool: PublicKey = contractData.pools.pop();
  //   let poolData = await program.account.pool.fetch(newPool);
  //   const [custody] = PublicKey.findProgramAddressSync(
  //     [
  //       Buffer.from("custody"),
  //       newPool.toBuffer(),
  //       usdcMint.toBuffer(),
  //     ],
  //     program.programId
  //   );
  //   console.log("custody", custody)
  //   const tx = await program.methods
  //     .reallocPool({ ratios: [{ target: new anchor.BN(60), min: new anchor.BN(40), max: new anchor.BN(70) }, { target: new anchor.BN(40), min: new anchor.BN(30), max: new anchor.BN(60) }], custodyKey: custody, poolName: poolData.name })
  //     .accounts({
  //       signer: localWallet.publicKey,
  //     })
  //     .signers([localWallet])
  //     .rpc();
  //     // 3zpMg2fibQ16JqGVuCNBz949nBXYaQm54QeLFdM4YRqzCZkYxSaGypPpWvxgmfkxMts43Kb6Bb6UR3c9iuoXAN3Y
  //   console.log("Your transaction signature1", tx);

  //   const tx2 = await program.methods
  //     .addCustody({ oracle: new PublicKey("5SSkXsEKQepHHAewytPVwdej4epN1nxgLVM84L4KXgy7"), poolName: poolData.name })
  //     .accounts({
  //       signer: localWallet.publicKey,
  //       custodyTokenMint: usdcMint
  //     })
  //     .signers([localWallet])
  //     .rpc();
  //     // 34Sa7YaBwkkxeNjKfuPuvU5poc18Ci6XmSLhqgmAKVi4y8Li3qrhusonqesHosavKJ8ZK6CyRsK2y6Pcy7suAPsw
  //   console.log("Your transaction signature2", tx2);

  // });

  // it("add_liquidity_usdc", async () => {
  //   // Add your test here.
  //   let newPool: PublicKey = contractData.pools.pop();
  //   let poolData = await program.account.pool.fetch(newPool);
  //   let custodies = []
  //   let oracles = [];
  //   for await (let custody of poolData.custodies) {

  //     let c = await program.account.custody.fetch(new PublicKey(custody))
  //     let ora = c.oracle
  //     console.log("c.fees", c.fees)
  //     console.log("custody:", custody, "oracle: ", ora);
  //     custodies.push({ pubkey: custody, isSigner: false, isWritable: true });
  //     oracles.push({ pubkey: ora, isSigner: false, isWritable: true });
  //   }

  //   const remainingAccounts = custodies.concat(oracles);

  //   const fundingAccount = getAssociatedTokenAddressSync(usdcMint, localWallet.publicKey)
  //   const [lpTokenMint] = PublicKey.findProgramAddressSync(
  //     [
  //       Buffer.from("lp_token_mint"),
  //       Buffer.from("SOL-USDC")
  //     ],
  //     program.programId
  //   );
  //   const [custodyTokenAccount] = PublicKey.findProgramAddressSync(
  //     [
  //       Buffer.from("custody_token_account"),
  //       newPool.toBuffer(),
  //       usdcMint.toBuffer()
  //     ],
  //     program.programId
  //   );
  //   const lpTokenAccount = getAssociatedTokenAddressSync(lpTokenMint, localWallet.publicKey);
  //   console.log("fundingAccount", fundingAccount, "lpTokenMint", lpTokenMint, "lpTokenAccount", lpTokenAccount, "custodyTokenAccount", custodyTokenAccount)
  //   const tx = await program.methods
  //     .addLiquidity({ amountIn: new anchor.BN(10000000), minLpAmountOut: new anchor.BN(10000000), poolName: "SOL-USDC" })
  //     .accounts({
  //       owner: localWallet.publicKey,
  //       fundingAccount: fundingAccount,
  //       custodyMint: usdcMint,
  //       custodyOracleAccount: new PublicKey("5SSkXsEKQepHHAewytPVwdej4epN1nxgLVM84L4KXgy7")
  //     }).remainingAccounts(remainingAccounts)
  //     .signers([localWallet])
  //     .rpc();
  //   console.log("Your transaction signature1", tx);

  // });

  // it("add_liquidity_sol", async () => {
  //   // Add your test here.
  //   let newPool: PublicKey = contractData.pools.pop();
  //   let poolData = await program.account.pool.fetch(newPool);
  //   let custodies = []
  //   let oracles = [];
  //   for await (let custody of poolData.custodies) {

  //     let c = await program.account.custody.fetch(new PublicKey(custody))
  //     let ora = c.oracle
  //     console.log("c.fees", c.tokenAccount.toBase58())
  //     console.log("custody:", custody, "oracle: ", ora);
  //     custodies.push({ pubkey: custody, isSigner: false, isWritable: true });
  //     oracles.push({ pubkey: ora, isSigner: false, isWritable: true });
  //   }

  //   const remainingAccounts = custodies.concat(oracles);

  //   const fundingAccount = getAssociatedTokenAddressSync(wsolMint, localWallet.publicKey)
  //   const [lpTokenMint] = PublicKey.findProgramAddressSync(
  //     [
  //       Buffer.from("lp_token_mint"),
  //       Buffer.from("SOL-USDC")
  //     ],
  //     program.programId
  //   );
  //   const [custodyTokenAccount] = PublicKey.findProgramAddressSync(
  //     [
  //       Buffer.from("custody_token_account"),
  //       newPool.toBuffer(),
  //       wsolMint.toBuffer()
  //     ],
  //     program.programId
  //   );
  //   const lpTokenAccount = getAssociatedTokenAddressSync(lpTokenMint, localWallet.publicKey);
  //   console.log("fundingAccount", fundingAccount, "lpTokenMint", lpTokenMint, "lpTokenAccount", lpTokenAccount, "custodyTokenAccount", custodyTokenAccount)
  //   const tx = await program.methods
  //     .addLiquidity({ amountIn: new anchor.BN(10000000), minLpAmountOut: new anchor.BN(10000000), poolName: "SOL-USDC" })
  //     .accounts({
  //       owner: localWallet.publicKey,
  //       fundingAccount: fundingAccount,
  //       custodyMint: wsolMint,
  //       custodyOracleAccount: new PublicKey("J83w4HKfqxwcq3BEMMkPFSppX3gqekLyLJBexebFVkix")
  //     }).remainingAccounts(remainingAccounts)
  //     .signers([localWallet])
  //     .rpc();
  //   console.log("Your transaction signature1", tx);

  // });

  it("open_option_call_sol", async () => {
    // Add your test here.
    let newPool: PublicKey = contractData.pools.pop();
    let poolData = await program.account.pool.fetch(newPool);
    const fundingAccount = getAssociatedTokenAddressSync(wsolMint, localWallet.publicKey)
    const [lpTokenMint] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("lp_token_mint"),
        Buffer.from("SOL-USDC")
      ],
      program.programId
    );
    const [custodyTokenAccount] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("custody_token_account"),
        newPool.toBuffer(),
        wsolMint.toBuffer()
      ],
      program.programId
    );
    const lpTokenAccount = getAssociatedTokenAddressSync(lpTokenMint, localWallet.publicKey);
    let wsolCustody: PublicKey;
    for await (let custody of poolData.custodies) {

      let c = await program.account.custody.fetch(new PublicKey(custody))
      let mint = c.mint
      if (mint.toBase58() == wsolMint?.toBase58()){
        wsolCustody = custody;
      }
    }
    const [optionDetail] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("option"),
        localWallet.publicKey.toBuffer(),
        new anchor.BN(2).toArrayLike(Buffer, "le", 8),
        newPool.toBuffer(),
        wsolCustody.toBuffer()
      ],
      program.programId
    );
    console.log("optionDetail", optionDetail, "wsolCustody", wsolCustody);
    const tx = await program.methods
      .openOption({ amount: new anchor.BN(1000000), strike: 130, expiredTime: new anchor.BN(1745221178), period: new anchor.BN(7), poolName: "SOL-USDC" })
      .accountsPartial({
        owner: localWallet.publicKey,
        fundingAccount: fundingAccount,
        custodyMint: wsolMint,
        payCustodyMint: wsolMint,
        custodyOracleAccount: new PublicKey("J83w4HKfqxwcq3BEMMkPFSppX3gqekLyLJBexebFVkix"),
        payCustodyOracleAccount: new PublicKey("J83w4HKfqxwcq3BEMMkPFSppX3gqekLyLJBexebFVkix"),
        lockedCustodyMint: wsolMint,
        optionDetail: optionDetail,
        pool: newPool,
        custody: wsolCustody
      })
      .signers([localWallet])
      .rpc(); // {skipPreflight: true} 

      // https://solscan.io/tx/4y5N2Pec7LtdzpsEDuBrG8WhzWdEUJ4CUp9ceE6k1fnxgNHZJGNEQ6z8SpBdiME9qjNFwtdxbi6nJF59eyTMEwLZ?cluster=devnet
      // https://solscan.io/tx/5CSCCx9kpJZXVs5jW38SScSiYN2myogLNoJa6GFXTDKMzgxuUWMWCwAAiWRa1CSXRLdFhXPMSuVvNpcfa3VrZwXG?cluster=devnet
    console.log("Your transaction signature1", tx);

  });
});


// const [userinfo] = PublicKey.findProgramAddressSync(
//   [
//     Buffer.from("user"),
//     userWallet.publicKey.toBuffer(),
//   ],
//   program.programId
// );

// const userInfo =
// await program.account.user.fetch(userinfo);
// console.log(userInfo.optionIndex.toNumber())
// const [detailAccount] = PublicKey.findProgramAddressSync(
//   [
//     Buffer.from("option"),
//     userWallet.publicKey.toBuffer(),
//     new anchor.BN(3).toArrayLike(Buffer, "le", 8),
//   ],
//   program.programId
// );
// console.log("detail account:", detailAccount.toBase58());