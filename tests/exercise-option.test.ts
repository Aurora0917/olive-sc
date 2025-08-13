import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { OptionContract } from "../target/types/option_contract";
import { expect } from "chai";
import {
  PublicKey,
  Keypair,
  SystemProgram,
} from "@solana/web3.js";
import {
  getAccount,
  getAssociatedTokenAddressSync,
  TOKEN_PROGRAM_ID,
  ASSOCIATED_TOKEN_PROGRAM_ID,
} from "@solana/spl-token";

describe("Exercise Option Test - Based on Rust Code", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.OptionContract as Program<OptionContract>;

  // Your existing mints and oracles
  const WSOLMint = new PublicKey("6fiDYq4uZgQQNUZVaBBcwu9jAUTWWBb7U8nmxt6BCaHY");
  const USDCMint = new PublicKey("Fe7yM1wqx5ySZmSHJjNzkLuvBCU8BEnYpmxcpGwwBkZq");
  const WSOL_ORACLE = new PublicKey("J83w4HKfqxwcq3BEMMkPFSppX3gqekLyLJBexebFVkix");
  
  const poolName = "SOL-USDC";
  const optionIndex = 2;
  
  let userWallet: Keypair;
  let userWSOLAccount: PublicKey;

  before(async () => {
    userWallet = provider.wallet.payer;
    userWSOLAccount = getAssociatedTokenAddressSync(WSOLMint, userWallet.publicKey);

    console.log("🎯 Test Setup Complete");
    console.log("User Wallet:", userWallet.publicKey.toBase58());
    console.log("User WSOL Account:", userWSOLAccount.toBase58());
    console.log("WSOL Mint:", WSOLMint.toBase58());
    console.log("WSOL Oracle:", WSOL_ORACLE.toBase58());
  });

  it("Should exercise option following exact Rust structure", async () => {
    console.log("\n🚀 Starting exercise option test...");

    // ====================================================================
    // STEP 1: Calculate all PDAs exactly as defined in Rust
    // ====================================================================
    
    const [contractPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("contract")],
      program.programId
    );

    const [poolPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("pool"), Buffer.from(poolName)],
      program.programId
    );

    const [transferAuthorityPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("transfer_authority")],
      program.programId
    );

    const [userPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("user_v3"), userWallet.publicKey.toBuffer()],
      program.programId
    );

    // For a WSOL call option:
    // - custody_mint = WSOLMint (the target price asset)
    // - locked_custody_mint = WSOLMint (the locked asset) 
    // Both custodies will be the same WSOL custody

    const [custodyPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("custody"), poolPDA.toBuffer(), WSOLMint.toBuffer()], // custody_mint
      program.programId
    );

    const [lockedCustodyPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("custody"), poolPDA.toBuffer(), WSOLMint.toBuffer()], // locked_custody_mint
      program.programId
    );

    // Option detail uses the custody (not locked_custody) in its derivation
    const [optionDetailPDA] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("option"),
        userWallet.publicKey.toBuffer(),
        new anchor.BN(optionIndex).toArrayLike(Buffer, "le", 8),
        poolPDA.toBuffer(),
        custodyPDA.toBuffer(), // Uses custody, not locked_custody
      ],
      program.programId
    );

    // Token account for locked custody
    const [lockedCustodyTokenAccountPDA] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("custody_token_account"),
        poolPDA.toBuffer(),
        WSOLMint.toBuffer(), // locked_custody_mint
      ],
      program.programId
    );

    console.log("\n📋 Calculated PDAs:");
    console.log("  Contract:", contractPDA.toBase58());
    console.log("  Pool:", poolPDA.toBase58());
    console.log("  Transfer Authority:", transferAuthorityPDA.toBase58());
    console.log("  User:", userPDA.toBase58());
    console.log("  Custody:", custodyPDA.toBase58());
    console.log("  Locked Custody:", lockedCustodyPDA.toBase58());
    console.log("  Option Detail:", optionDetailPDA.toBase58());
    console.log("  Locked Custody Token Account:", lockedCustodyTokenAccountPDA.toBase58());
    console.log("  Same Custody?", custodyPDA.equals(lockedCustodyPDA));

    // ====================================================================
    // STEP 2: Validate initial state
    // ====================================================================

    const initialUserBalance = await getAccount(provider.connection, userWSOLAccount);
    const initialCustodyBalance = await getAccount(provider.connection, lockedCustodyTokenAccountPDA);
    const initialOptionData = await program.account.optionDetail.fetch(optionDetailPDA);

    console.log("\n📊 Initial State:");
    console.log("  User WSOL Balance:", initialUserBalance.amount.toString());
    console.log("  Custody Token Balance:", initialCustodyBalance.amount.toString());
    console.log("  Option Amount:", initialOptionData.amount.toString());
    console.log("  Option Strike Price:", initialOptionData.strikePrice);
    console.log("  Option Valid:", initialOptionData.valid);
    console.log("  Option Exercised:", initialOptionData.exercised.toString());

    // Validate option is exercisable
    expect(initialOptionData.valid).to.be.true;
    expect(initialOptionData.exercised.toNumber()).to.equal(0);

    // ====================================================================
    // STEP 3: Validate the key constraint from Rust
    // ====================================================================

    // Rust constraint: funding_account.mint == locked_custody.mint
    const fundingAccountInfo = await getAccount(provider.connection, userWSOLAccount);
    const lockedCustodyData = await program.account.custody.fetch(lockedCustodyPDA);

    console.log("\n🔍 Constraint Validation:");
    console.log("  Funding Account Mint:", fundingAccountInfo.mint.toBase58());
    console.log("  Locked Custody Mint:", lockedCustodyData.mint.toBase58());
    console.log("  Constraint Satisfied:", fundingAccountInfo.mint.equals(lockedCustodyData.mint));

    if (!fundingAccountInfo.mint.equals(lockedCustodyData.mint)) {
      throw new Error("Constraint violation: funding_account.mint != locked_custody.mint");
    }

    // ====================================================================
    // STEP 4: Execute transaction with ALL required accounts
    // ====================================================================

    try {
      console.log("\n⚡ Executing exercise option transaction...");

      const exerciseOptionTx = await program.methods
        .exerciseOption({
          optionIndex: new anchor.BN(optionIndex),
          poolName: poolName,
        })
        .accounts({
          // Every account from the Rust struct
          owner: userWallet.publicKey,
          fundingAccount: userWSOLAccount,
          transferAuthority: transferAuthorityPDA,
          contract: contractPDA,
          pool: poolPDA,
          custody: custodyPDA,
          user: userPDA,
          optionDetail: optionDetailPDA,
          lockedCustody: lockedCustodyPDA,
          lockedCustodyTokenAccount: lockedCustodyTokenAccountPDA,
          lockedOracle: WSOL_ORACLE,
          custodyMint: WSOLMint,        // custody_mint
          lockedCustodyMint: WSOLMint,  // locked_custody_mint
          tokenProgram: TOKEN_PROGRAM_ID,
          associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
        })
        .signers([userWallet])
        .rpc();

      console.log("Exercise option transaction successful!");
      console.log("Transaction signature:", exerciseOptionTx);

      // Wait for confirmation
      await provider.connection.confirmTransaction(exerciseOptionTx, "confirmed");

      // ====================================================================
      // STEP 5: Verify results
      // ====================================================================

      const finalUserBalance = await getAccount(provider.connection, userWSOLAccount);
      const finalCustodyBalance = await getAccount(provider.connection, lockedCustodyTokenAccountPDA);
      const finalOptionData = await program.account.optionDetail.fetch(optionDetailPDA);

      // Calculate changes
      const userProfit = finalUserBalance.amount - initialUserBalance.amount;
      const custodyDecrease = initialCustodyBalance.amount - finalCustodyBalance.amount;

      console.log("\n📈 Results:");
      console.log("  User Profit:", userProfit.toString());
      console.log("  Custody Decrease:", custodyDecrease.toString());
      console.log("  Option Profit:", finalOptionData.profit.toString());
      console.log("  Option Valid:", finalOptionData.valid);
      console.log("  Option Exercised:", finalOptionData.exercised.toString());

      // Assertions
      expect(finalOptionData.valid).to.be.false;
      expect(finalOptionData.exercised.toNumber()).to.be.greaterThan(0);
      expect(finalOptionData.profit.toNumber()).to.be.greaterThan(0);
      expect(userProfit).to.be.greaterThan(0n);
      expect(userProfit).to.equal(custodyDecrease);
      expect(userProfit).to.equal(BigInt(finalOptionData.profit.toString()));

      console.log("🎉 Exercise option test completed successfully!");

    } catch (error) {
      console.error("\n❌ Exercise option failed:");
      console.error("Error:", error.message);
      
      if (error.logs) {
        console.error("\nTransaction logs:");
        error.logs.forEach((log, i) => console.error(`  ${i}: ${log}`));
      }

      // Additional debugging
      console.log("\n🔍 Debug Information:");
      
      // Check mint account ownership
      const wsolMintInfo = await provider.connection.getAccountInfo(WSOLMint);
      console.log("WSOL Mint Info:");
      console.log("  Owner:", wsolMintInfo?.owner.toBase58());
      console.log("  Is Token Program:", wsolMintInfo?.owner.equals(TOKEN_PROGRAM_ID));

      // Check custody info
      const custodyData = await program.account.custody.fetch(custodyPDA);
      console.log("Custody Info:");
      console.log("  Mint:", custodyData.mint.toBase58());
      console.log("  Oracle:", custodyData.oracle.toBase58());

      throw error;
    }
  });

  it("Should validate all accounts exist", async () => {
    console.log("\n🔍 Validating required accounts...");
    
    const [contractPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("contract")],
      program.programId
    );

    const [poolPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("pool"), Buffer.from(poolName)],
      program.programId
    );

    const [userPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("user_v3"), userWallet.publicKey.toBuffer()],
      program.programId
    );

    const [custodyPDA] = PublicKey.findProgramAddressSync(
      [Buffer.from("custody"), poolPDA.toBuffer(), WSOLMint.toBuffer()],
      program.programId
    );

    const [optionDetailPDA] = PublicKey.findProgramAddressSync(
      [
        Buffer.from("option"),
        userWallet.publicKey.toBuffer(),
        new anchor.BN(optionIndex).toArrayLike(Buffer, "le", 8),
        poolPDA.toBuffer(),
        custodyPDA.toBuffer(),
      ],
      program.programId
    );

    // Validate each account
    try {
      await program.account.contract.fetch(contractPDA);
      console.log("Contract exists");
    } catch (e) {
      throw new Error(`Contract missing: ${e.message}`);
    }

    try {
      const poolData = await program.account.pool.fetch(poolPDA);
      console.log("Pool exists:", poolData.name);
    } catch (e) {
      throw new Error(`Pool missing: ${e.message}`);
    }

    try {
      const userData = await program.account.user.fetch(userPDA);
      console.log("User exists, option index:", userData.optionIndex.toString());
      expect(userData.optionIndex.toNumber()).to.be.gte(optionIndex);
    } catch (e) {
      throw new Error(`User missing: ${e.message}`);
    }

    try {
      const custodyData = await program.account.custody.fetch(custodyPDA);
      console.log("Custody exists");
      console.log("  Mint:", custodyData.mint.toBase58());
      console.log("  Oracle:", custodyData.oracle.toBase58());
      console.log("  Token Locked:", custodyData.tokenLocked.toString());
    } catch (e) {
      throw new Error(`Custody missing: ${e.message}`);
    }

    try {
      const optionData = await program.account.optionDetail.fetch(optionDetailPDA);
      console.log("Option Detail exists");
      console.log("  Valid:", optionData.valid);
      console.log("  Amount:", optionData.amount.toString());
      console.log("  Strike Price:", optionData.strikePrice);
      
      expect(optionData.valid).to.be.true;
      expect(optionData.exercised.toNumber()).to.equal(0);
    } catch (e) {
      throw new Error(`Option Detail missing: ${e.message}`);
    }

    console.log("All required accounts validated");
  });
});