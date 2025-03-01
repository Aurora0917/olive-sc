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

describe("option-contract", () => {
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

  // it("Is initialized!", async () => {
  //   // Add your test here.
  //   const [lp, lpBump] = PublicKey.findProgramAddressSync(
  //     [Buffer.from("lp")],
  //     program.programId
  //   );
  //   console.log(lp, lpBump)
  //   const tx = await program.methods
  //     .initialize(lpBump)
  //     .accountsPartial({ wsolMint: wsolMint, usdcMint: usdcMint, lp:lp })
  //     .signers([localWallet])
  //     .rpc();
  //   console.log("Your transaction signature", tx);
  // });
  it("sell option!", async () => {
    // Add your test here.
    console.log(userWallet.publicKey.toBase58());
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
    const [detailAccount] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("option"),
        userWallet.publicKey.toBuffer(),
        new anchor.BN(3).toArrayLike(Buffer, "le", 8),
      ],
      program.programId
    );
    console.log("detail account:", detailAccount.toBase58());

    // const tx = await program.methods
    //   .sellOption(
    //     new anchor.BN(2),
    //     new anchor.BN(1),
    //     150.3,
    //     new anchor.BN(14),
    //     new anchor.BN(1745727408),
    //     true,
    //     false
    //   )
    //   .accounts({
    //     signer: userWallet.publicKey,
    //     wsolMint: wsolMint,
    //     usdcMint: usdcMint,
    //     pythPriceAccount: SOL_PYTH_FEED,
    //   })
    //   .signers([userWallet])
    //   .rpc();

    // console.log("Your transaction signature", tx);
  });
});
