use anchor_lang::prelude::*;
use anchor_lang::solana_program::program_option::COption;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::metadata::{
    create_metadata_accounts_v3, mpl_token_metadata::types::DataV2, CreateMetadataAccountsV3,
    Metadata,
};
use anchor_spl::token::spl_token::state::AccountState;
use anchor_spl::token::{
    self, Burn, FreezeAccount, Mint, MintTo, SetAuthority, Token, TokenAccount, Transfer,
};

declare_id!("2bizvF84cNRhEQdZPH7nBHCyA712uArhNe11CTg43Cm1");

const INITIAL_PRICE_MICRO_USDC: u64 = 1_000_000;
const MAX_NAME_LEN: usize = 32;
const MAX_DESCRIPTION_LEN: usize = 200;
const MAX_URL_LEN: usize = 200;
const MAX_IMAGE_DATA_LEN: usize = 1024; // temporaire pour Playground/dev
const LOCK_AMOUNT_BLOCK: u64 = 1_000;
const NFT_SYMBOL: &str = "1MB";

// TODO Phase 2D — Metaplex Inscription :
// Actuellement name, description, image_data, url sont stockés dans PixelAccount.
// Migrer vers Metaplex Inscription Program (mpl-inscription) pour un stockage
// 100% on-chain indépendant du programme.
// Instruction future : initialize_inscription + write_data vers InscriptionAccount,
// référencé depuis PixelAccount via un champ inscription_account: Pubkey.

#[program]
pub mod one_million_block {
    use super::*;

    pub fn initialize_billboard(
        ctx: Context<InitializeBillboard>,
        wallet_initial_buys: Pubkey,
        wallet_rebuy_fees: Pubkey,
        block_token_mint: Pubkey,
    ) -> Result<()> {
        let billboard = &mut ctx.accounts.billboard;
        billboard.total_pixels_sold = 0;
        billboard.total_pixels_locked = 0;
        billboard.total_block_burned = 0;
        billboard.total_usdc_revenue = 0;
        billboard.wallet_initial_buys = wallet_initial_buys;
        billboard.wallet_rebuy_fees = wallet_rebuy_fees;
        billboard.block_token_mint = block_token_mint;
        billboard.bump = ctx.bumps.billboard;
        Ok(())
    }

    /// Phase 2A — Crée un vrai NFT Metaplex pour le pixel acheté.
    ///
    /// Flux :
    ///   1. Transfer 1 USDC → wallet_initial_buys
    ///   2. CPI → Metaplex Token Metadata : CreateMetadataAccountsV3
    ///      - name = nom du pixel, symbol = "1MB", uri = "" (données on-chain dans PixelAccount)
    ///      - update_authority = PDA billboard (le programme contrôle les métadonnées)
    ///   3. Mint 1 token NFT → ATA de l'acheteur
    ///   4. Révoquer la mint_authority (supply fixée à 1 définitivement)
    ///   5. Stocker pixel.nft_mint
    ///
    /// Prérequis client : le nft_mint doit être créé en amont avec
    ///   mint_authority = signer, freeze_authority = billboard PDA.
    pub fn buy_pixel(
        ctx: Context<BuyPixel>,
        x: u16,
        y: u16,
        name: String,
        description: String,
        image_data: Vec<u8>,
        url: String,
    ) -> Result<()> {
        require!(x < 1000, ErrorCode::InvalidCoordinate);
        require!(y < 1000, ErrorCode::InvalidCoordinate);
        validate_metadata(&name, &description, &image_data, &url)?;

        require_keys_eq!(
            ctx.accounts.usdc_destination.owner,
            ctx.accounts.billboard.wallet_initial_buys,
            ErrorCode::InvalidInitialBuyDestination
        );
        require_keys_eq!(
            ctx.accounts.buyer_usdc.mint,
            ctx.accounts.usdc_mint.key(),
            ErrorCode::InvalidUsdcMint
        );
        require_keys_eq!(
            ctx.accounts.usdc_destination.mint,
            ctx.accounts.usdc_mint.key(),
            ErrorCode::InvalidUsdcMint
        );

        // 1) Transfer 1 USDC → wallet_initial_buys (100% au trésor initial)
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.buyer_usdc.to_account_info(),
                    to: ctx.accounts.usdc_destination.to_account_info(),
                    authority: ctx.accounts.signer.to_account_info(),
                },
            ),
            INITIAL_PRICE_MICRO_USDC,
        )?;

        // Seeds du PDA billboard pour signer les CPIs en son nom
        let billboard_bump = ctx.accounts.billboard.bump;
        let billboard_seeds: &[&[u8]] = &[b"billboard", &[billboard_bump]];
        let signer_seeds = &[billboard_seeds];

        // 2) CPI → Metaplex Token Metadata : CreateMetadataAccountsV3
        // L'update_authority du NFT est le PDA billboard (permet update_metadata_v2 futur).
        // uri = "" car les données réelles sont dans PixelAccount (futur : Inscription).
        create_metadata_accounts_v3(
            CpiContext::new_with_signer(
                ctx.accounts.metadata_program.to_account_info(),
                CreateMetadataAccountsV3 {
                    metadata: ctx.accounts.nft_metadata.to_account_info(),
                    mint: ctx.accounts.nft_mint.to_account_info(),
                    mint_authority: ctx.accounts.signer.to_account_info(),
                    payer: ctx.accounts.signer.to_account_info(),
                    update_authority: ctx.accounts.billboard.to_account_info(),
                    system_program: ctx.accounts.system_program.to_account_info(),
                    rent: ctx.accounts.rent.to_account_info(),
                },
                signer_seeds,
            ),
            DataV2 {
                name: name.clone(),
                symbol: NFT_SYMBOL.to_string(),
                uri: String::new(),
                seller_fee_basis_points: 0,
                creators: None,
                collection: None,
                uses: None,
            },
            true, // is_mutable : le nom/symbol peuvent être mis à jour plus tard
            true, // update_authority_is_signer
            None, // collection_details
        )?;

        // 3) Mint 1 token NFT → ATA de l'acheteur
        // Le signer est la mint_authority (défini lors de la création du mint côté client).
        token::mint_to(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.nft_mint.to_account_info(),
                    to: ctx.accounts.buyer_nft_token.to_account_info(),
                    authority: ctx.accounts.signer.to_account_info(),
                },
            ),
            1,
        )?;

        // 4) Révoquer la mint_authority → supply max = 1 (vrai NFT non-fongible)
        token::set_authority(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                SetAuthority {
                    current_authority: ctx.accounts.signer.to_account_info(),
                    account_or_mint: ctx.accounts.nft_mint.to_account_info(),
                },
            ),
            token::spl_token::instruction::AuthorityType::MintTokens,
            None,
        )?;

        // 5) Init PixelAccount
        let pixel = &mut ctx.accounts.pixel;
        let billboard = &mut ctx.accounts.billboard;

        pixel.x = x;
        pixel.y = y;
        pixel.owner = ctx.accounts.signer.key();
        pixel.current_price = INITIAL_PRICE_MICRO_USDC;
        pixel.rebuy_count = 0;
        pixel.locked = false;
        pixel.locked_at_block = 0;
        pixel.nft_mint = ctx.accounts.nft_mint.key();
        pixel.name = name;
        pixel.description = description;
        pixel.image_data = image_data;
        pixel.url = url;
        pixel.bump = ctx.bumps.pixel;

        billboard.total_pixels_sold = billboard
            .total_pixels_sold
            .checked_add(1)
            .ok_or(ErrorCode::MathOverflow)?;

        Ok(())
    }

    /// Phase 2B — Rachète un pixel : transfère USDC + transfère le NFT SPL.
    ///
    /// Flux :
    ///   1. Calcul nouveau prix (×2), split 95% vendeur / 5% protocole
    ///   2. Transfer USDC : buyer → seller (95%) + buyer → protocol (5%)
    ///   3. CPI Token Transfer NFT : seller_nft_token → buyer_nft_token
    ///      Le PDA billboard est le delegate du seller_nft_token (approve côté client).
    ///   4. Mise à jour pixel.owner + métadonnées
    ///
    /// Prérequis client : le vendeur doit appeler approve() sur son ATA NFT
    ///   en désignant le billboard PDA comme delegate, avant d'appeler rebuy_pixel.
    pub fn rebuy_pixel(
        ctx: Context<RebuyPixel>,
        x: u16,
        y: u16,
        new_name: String,
        new_description: String,
        new_image_data: Vec<u8>,
        new_url: String,
    ) -> Result<()> {
        validate_metadata(&new_name, &new_description, &new_image_data, &new_url)?;

        require!(x < 1000, ErrorCode::InvalidCoordinate);
        require!(y < 1000, ErrorCode::InvalidCoordinate);

        let billboard_bump = ctx.accounts.billboard.bump;
        let protocol_wallet = ctx.accounts.billboard.wallet_rebuy_fees;
        let billboard_info = ctx.accounts.billboard.to_account_info();

        let pixel = &mut ctx.accounts.pixel;
        let signer = &ctx.accounts.signer;

        require!(pixel.x == x, ErrorCode::PixelCoordinateMismatch);
        require!(pixel.y == y, ErrorCode::PixelCoordinateMismatch);
        require!(!pixel.locked, ErrorCode::PixelLocked);
        require!(pixel.owner != signer.key(), ErrorCode::AlreadyOwner);

        require_keys_eq!(
            ctx.accounts.buyer_usdc.owner,
            signer.key(),
            ErrorCode::InvalidBuyerTokenOwner
        );
        require_keys_eq!(
            ctx.accounts.seller_usdc.owner,
            pixel.owner,
            ErrorCode::InvalidSellerTokenOwner
        );
        require_keys_eq!(
            ctx.accounts.protocol_usdc.owner,
            protocol_wallet,
            ErrorCode::InvalidProtocolDestination
        );
        require_keys_eq!(
            ctx.accounts.buyer_usdc.mint,
            ctx.accounts.usdc_mint.key(),
            ErrorCode::InvalidUsdcMint
        );
        require_keys_eq!(
            ctx.accounts.seller_usdc.mint,
            ctx.accounts.usdc_mint.key(),
            ErrorCode::InvalidUsdcMint
        );
        require_keys_eq!(
            ctx.accounts.protocol_usdc.mint,
            ctx.accounts.usdc_mint.key(),
            ErrorCode::InvalidUsdcMint
        );
        require_keys_eq!(
            ctx.accounts.nft_mint.key(),
            pixel.nft_mint,
            ErrorCode::InvalidNftMint
        );
        require_keys_eq!(
            ctx.accounts.seller_nft_token.mint,
            pixel.nft_mint,
            ErrorCode::InvalidNftMint
        );
        require_keys_eq!(
            ctx.accounts.seller_nft_token.owner,
            pixel.owner,
            ErrorCode::InvalidNftTokenOwner
        );
        require_keys_eq!(
            ctx.accounts.buyer_nft_token.mint,
            pixel.nft_mint,
            ErrorCode::InvalidNftMint
        );
        require!(
            ctx.accounts.seller_nft_token.delegate == COption::Some(ctx.accounts.billboard.key()),
            ErrorCode::MissingNftDelegateApproval
        );
        require!(
            ctx.accounts.seller_nft_token.delegated_amount >= 1,
            ErrorCode::MissingNftDelegateApproval
        );

        let new_price = pixel
            .current_price
            .checked_mul(2)
            .ok_or(ErrorCode::MathOverflow)?;
        let seller_amount = new_price
            .checked_mul(95)
            .ok_or(ErrorCode::MathOverflow)?
            .checked_div(100)
            .ok_or(ErrorCode::MathOverflow)?;
        let protocol_amount = new_price
            .checked_sub(seller_amount)
            .ok_or(ErrorCode::MathOverflow)?;

        // 1) Transfer USDC → vendeur (95%)
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.buyer_usdc.to_account_info(),
                    to: ctx.accounts.seller_usdc.to_account_info(),
                    authority: signer.to_account_info(),
                },
            ),
            seller_amount,
        )?;

        // 2) Transfer USDC → protocole (5%)
        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.buyer_usdc.to_account_info(),
                    to: ctx.accounts.protocol_usdc.to_account_info(),
                    authority: signer.to_account_info(),
                },
            ),
            protocol_amount,
        )?;

        // 3) Transfer NFT : seller_nft_token → buyer_nft_token
        // Le PDA billboard signe ce transfer car le vendeur lui a délégué via approve().
        // Cela évite que le vendeur doive co-signer la transaction de l'acheteur.
        let billboard_seeds: &[&[u8]] = &[b"billboard", &[billboard_bump]];
        let signer_seeds = &[billboard_seeds];

        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.seller_nft_token.to_account_info(),
                    to: ctx.accounts.buyer_nft_token.to_account_info(),
                    authority: billboard_info,
                },
                signer_seeds,
            ),
            1,
        )?;

        pixel.owner = signer.key();
        pixel.current_price = new_price;
        pixel.rebuy_count = pixel
            .rebuy_count
            .checked_add(1)
            .ok_or(ErrorCode::MathOverflow)?;
        pixel.name = new_name;
        pixel.description = new_description;
        pixel.image_data = new_image_data;
        pixel.url = new_url;

        {
            let billboard = &mut ctx.accounts.billboard;
            billboard.total_usdc_revenue = billboard
                .total_usdc_revenue
                .checked_add(protocol_amount)
                .ok_or(ErrorCode::MathOverflow)?;
        }

        Ok(())
    }

    /// Phase 2C — Verrouille le pixel de façon irréversible.
    ///
    /// Flux :
    ///   1. Burn 1 000 $BLOCK (raw = 1 000 × 10^decimals)
    ///   2. CPI → Token Program : freeze_account sur owner_nft_token
    ///      Le PDA billboard est la freeze_authority du NFT mint (défini au mint dans buy_pixel).
    ///      Le compte token est gelé → NFT intransférable définitivement.
    ///   3. pixel.locked = true (pas d'instruction unfreeze dans ce programme — jamais)
    pub fn lock_pixel(ctx: Context<LockPixel>, x: u16, y: u16) -> Result<()> {
        require!(x < 1000, ErrorCode::InvalidCoordinate);
        require!(y < 1000, ErrorCode::InvalidCoordinate);

        let billboard_bump = ctx.accounts.billboard.bump;
        let block_mint = ctx.accounts.billboard.block_token_mint;
        let billboard_info = ctx.accounts.billboard.to_account_info();

        let pixel = &mut ctx.accounts.pixel;
        let owner = &ctx.accounts.owner;

        require!(pixel.x == x, ErrorCode::PixelCoordinateMismatch);
        require!(pixel.y == y, ErrorCode::PixelCoordinateMismatch);
        require!(!pixel.locked, ErrorCode::PixelAlreadyLocked);
        require_keys_eq!(pixel.owner, owner.key(), ErrorCode::Unauthorized);
        require_keys_eq!(
            block_mint,
            ctx.accounts.block_token_mint.key(),
            ErrorCode::InvalidBlockMint
        );
        require_keys_eq!(
            ctx.accounts.owner_block_token.owner,
            owner.key(),
            ErrorCode::InvalidBlockTokenOwner
        );
        require_keys_eq!(
            ctx.accounts.owner_block_token.mint,
            ctx.accounts.block_token_mint.key(),
            ErrorCode::InvalidBlockMint
        );
        require_keys_eq!(
            ctx.accounts.nft_mint.key(),
            pixel.nft_mint,
            ErrorCode::InvalidNftMint
        );
        require_keys_eq!(
            ctx.accounts.owner_nft_token.mint,
            pixel.nft_mint,
            ErrorCode::InvalidNftMint
        );
        require_keys_eq!(
            ctx.accounts.owner_nft_token.owner,
            owner.key(),
            ErrorCode::InvalidNftTokenOwner
        );
        require!(
            ctx.accounts.owner_nft_token.state != AccountState::Frozen,
            ErrorCode::PixelAlreadyLocked
        );
        require!(
            ctx.accounts.nft_mint.freeze_authority == COption::Some(ctx.accounts.billboard.key()),
            ErrorCode::InvalidNftFreezeAuthority
        );

        // 1) Burn $BLOCK (raw = LOCK_AMOUNT_BLOCK × 10^decimals)
        let decimals = ctx.accounts.block_token_mint.decimals;
        let decimals_factor = 10u64
            .checked_pow(decimals as u32)
            .ok_or(ErrorCode::MathOverflow)?;
        let burn_amount_raw = LOCK_AMOUNT_BLOCK
            .checked_mul(decimals_factor)
            .ok_or(ErrorCode::MathOverflow)?;

        require!(
            ctx.accounts.owner_block_token.amount >= burn_amount_raw,
            ErrorCode::InsufficientBlockBalance
        );

        token::burn(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.block_token_mint.to_account_info(),
                    from: ctx.accounts.owner_block_token.to_account_info(),
                    authority: owner.to_account_info(),
                },
            ),
            burn_amount_raw,
        )?;

        // 2) Freeze le compte token NFT du propriétaire
        // Le PDA billboard est la freeze_authority (défini lors du createInitializeMintInstruction).
        // Après ce freeze : le NFT ne peut plus être transféré ni brûlé — irréversible.
        let billboard_seeds: &[&[u8]] = &[b"billboard", &[billboard_bump]];
        let signer_seeds = &[billboard_seeds];

        token::freeze_account(CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            FreezeAccount {
                account: ctx.accounts.owner_nft_token.to_account_info(),
                mint: ctx.accounts.nft_mint.to_account_info(),
                authority: billboard_info,
            },
            signer_seeds,
        ))?;

        pixel.locked = true;
        pixel.locked_at_block = Clock::get()?.slot;

        {
            let billboard = &mut ctx.accounts.billboard;
            billboard.total_pixels_locked = billboard
                .total_pixels_locked
                .checked_add(1)
                .ok_or(ErrorCode::MathOverflow)?;
            billboard.total_block_burned = billboard
                .total_block_burned
                .checked_add(burn_amount_raw)
                .ok_or(ErrorCode::MathOverflow)?;
        }

        Ok(())
    }

    /// Mise à jour des métadonnées on-chain (autorisée même après lock).
    pub fn update_metadata(
        ctx: Context<UpdateMetadata>,
        x: u16,
        y: u16,
        name: String,
        description: String,
        image_data: Vec<u8>,
        url: String,
    ) -> Result<()> {
        validate_metadata(&name, &description, &image_data, &url)?;
        require!(x < 1000, ErrorCode::InvalidCoordinate);
        require!(y < 1000, ErrorCode::InvalidCoordinate);

        let pixel = &mut ctx.accounts.pixel;
        require!(pixel.x == x, ErrorCode::PixelCoordinateMismatch);
        require!(pixel.y == y, ErrorCode::PixelCoordinateMismatch);
        require_keys_eq!(
            pixel.owner,
            ctx.accounts.owner.key(),
            ErrorCode::Unauthorized
        );

        pixel.name = name;
        pixel.description = description;
        pixel.image_data = image_data;
        pixel.url = url;

        Ok(())
    }
}

fn validate_metadata(
    name: &String,
    description: &String,
    image_data: &Vec<u8>,
    url: &String,
) -> Result<()> {
    require!(name.len() <= MAX_NAME_LEN, ErrorCode::NameTooLong);
    require!(
        description.len() <= MAX_DESCRIPTION_LEN,
        ErrorCode::DescriptionTooLong
    );
    require!(url.len() <= MAX_URL_LEN, ErrorCode::UrlTooLong);
    require!(
        image_data.len() <= MAX_IMAGE_DATA_LEN,
        ErrorCode::ImageDataTooLarge
    );
    Ok(())
}

// ─────────────────────────────────────────────
// ACCOUNT CONTEXTS
// ─────────────────────────────────────────────

#[derive(Accounts)]
pub struct InitializeBillboard<'info> {
    #[account(
        init,
        payer = signer,
        space = 8 + BillboardAccount::LEN,
        seeds = [b"billboard"],
        bump
    )]
    pub billboard: Account<'info, BillboardAccount>,

    #[account(mut)]
    pub signer: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(x: u16, y: u16)]
pub struct BuyPixel<'info> {
    #[account(
        mut,
        seeds = [b"billboard"],
        bump = billboard.bump
    )]
    pub billboard: Account<'info, BillboardAccount>,

    #[account(
        init,
        payer = signer,
        space = 8 + PixelAccount::LEN,
        seeds = [
            b"pixel".as_ref(),
            x.to_le_bytes().as_ref(),
            y.to_le_bytes().as_ref()
        ],
        bump
    )]
    pub pixel: Account<'info, PixelAccount>,

    #[account(mut)]
    pub signer: Signer<'info>,

    /// Compte USDC de l'acheteur (source du paiement)
    #[account(mut, token::authority = signer)]
    pub buyer_usdc: Account<'info, TokenAccount>,

    /// Destination USDC (wallet_initial_buys)
    #[account(mut)]
    pub usdc_destination: Account<'info, TokenAccount>,

    pub usdc_mint: Account<'info, Mint>,

    /// Mint NFT de ce pixel (créé en amont avec decimals=0, freeze_authority=billboard PDA)
    #[account(mut)]
    pub nft_mint: Account<'info, Mint>,

    /// Metadata account Metaplex (PDA dérivé par Metaplex, adresse calculée côté client)
    /// CHECK: adresse validée par le CPI Metaplex Token Metadata
    #[account(mut)]
    pub nft_metadata: UncheckedAccount<'info>,

    /// ATA de l'acheteur pour le NFT mint (créé si inexistant)
    #[account(
        init_if_needed,
        payer = signer,
        associated_token::mint = nft_mint,
        associated_token::authority = signer
    )]
    pub buyer_nft_token: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub metadata_program: Program<'info, Metadata>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(x: u16, y: u16)]
pub struct RebuyPixel<'info> {
    #[account(
        mut,
        seeds = [b"billboard"],
        bump = billboard.bump
    )]
    pub billboard: Account<'info, BillboardAccount>,

    #[account(
        mut,
        seeds = [
            b"pixel".as_ref(),
            x.to_le_bytes().as_ref(),
            y.to_le_bytes().as_ref()
        ],
        bump = pixel.bump
    )]
    pub pixel: Account<'info, PixelAccount>,

    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(mut, token::authority = signer)]
    pub buyer_usdc: Account<'info, TokenAccount>,

    #[account(mut)]
    pub seller_usdc: Account<'info, TokenAccount>,

    #[account(mut)]
    pub protocol_usdc: Account<'info, TokenAccount>,

    pub usdc_mint: Account<'info, Mint>,

    /// Mint du NFT de ce pixel
    #[account(mut)]
    pub nft_mint: Account<'info, Mint>,

    /// Compte token NFT du vendeur (doit avoir pre-approuvé le billboard PDA comme delegate)
    #[account(mut)]
    pub seller_nft_token: Account<'info, TokenAccount>,

    /// ATA de l'acheteur pour le NFT (créé si inexistant)
    #[account(
        init_if_needed,
        payer = signer,
        associated_token::mint = nft_mint,
        associated_token::authority = signer
    )]
    pub buyer_nft_token: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(x: u16, y: u16)]
pub struct LockPixel<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        seeds = [b"billboard"],
        bump = billboard.bump
    )]
    pub billboard: Account<'info, BillboardAccount>,

    #[account(
        mut,
        seeds = [
            b"pixel".as_ref(),
            x.to_le_bytes().as_ref(),
            y.to_le_bytes().as_ref()
        ],
        bump = pixel.bump
    )]
    pub pixel: Account<'info, PixelAccount>,

    #[account(
        constraint = block_token_mint.key() == billboard.block_token_mint @ ErrorCode::InvalidBlockMint
    )]
    pub block_token_mint: Account<'info, Mint>,

    #[account(
        mut,
        constraint = owner_block_token.owner == owner.key() @ ErrorCode::InvalidBlockTokenOwner,
        constraint = owner_block_token.mint == block_token_mint.key() @ ErrorCode::InvalidBlockMint
    )]
    pub owner_block_token: Account<'info, TokenAccount>,

    /// Mint du NFT de ce pixel (la freeze_authority est le PDA billboard)
    #[account(mut)]
    pub nft_mint: Account<'info, Mint>,

    /// Compte token NFT du propriétaire (sera gelé irréversiblement)
    #[account(
        mut,
        constraint = owner_nft_token.owner == owner.key() @ ErrorCode::InvalidNftTokenOwner,
        constraint = owner_nft_token.mint == nft_mint.key() @ ErrorCode::InvalidNftMint
    )]
    pub owner_nft_token: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(x: u16, y: u16)]
pub struct UpdateMetadata<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        seeds = [
            b"pixel".as_ref(),
            x.to_le_bytes().as_ref(),
            y.to_le_bytes().as_ref()
        ],
        bump = pixel.bump
    )]
    pub pixel: Account<'info, PixelAccount>,
}

// ─────────────────────────────────────────────
// DATA ACCOUNTS
// ─────────────────────────────────────────────

#[account]
pub struct BillboardAccount {
    pub total_pixels_sold: u32,
    pub total_pixels_locked: u32,
    pub total_block_burned: u64,
    pub total_usdc_revenue: u64,
    pub wallet_initial_buys: Pubkey,
    pub wallet_rebuy_fees: Pubkey,
    pub block_token_mint: Pubkey,
    pub bump: u8,
}

impl BillboardAccount {
    pub const LEN: usize = 4 + 4 + 8 + 8 + 32 + 32 + 32 + 1;
}

#[account]
pub struct PixelAccount {
    pub x: u16,
    pub y: u16,
    pub owner: Pubkey,
    pub current_price: u64,
    pub rebuy_count: u8,
    pub locked: bool,
    pub locked_at_block: u64,
    pub nft_mint: Pubkey,
    pub name: String,
    pub description: String,
    pub image_data: Vec<u8>,
    pub url: String,
    pub bump: u8,
}

impl PixelAccount {
    pub const LEN: usize = 2
        + 2
        + 32
        + 8
        + 1
        + 1
        + 8
        + 32
        + 4
        + MAX_NAME_LEN
        + 4
        + MAX_DESCRIPTION_LEN
        + 4
        + MAX_IMAGE_DATA_LEN
        + 4
        + MAX_URL_LEN
        + 1;
}

// ─────────────────────────────────────────────
// ERROR CODES
// ─────────────────────────────────────────────

#[error_code]
pub enum ErrorCode {
    #[msg("Invalid coordinate. Must be between 0 and 999.")]
    InvalidCoordinate,
    #[msg("Name too long.")]
    NameTooLong,
    #[msg("Description too long.")]
    DescriptionTooLong,
    #[msg("URL too long.")]
    UrlTooLong,
    #[msg("Image data too large.")]
    ImageDataTooLarge,
    #[msg("Invalid USDC mint.")]
    InvalidUsdcMint,
    #[msg("Invalid initial buy destination.")]
    InvalidInitialBuyDestination,
    #[msg("Invalid seller token owner.")]
    InvalidSellerTokenOwner,
    #[msg("Invalid buyer token owner.")]
    InvalidBuyerTokenOwner,
    #[msg("Invalid protocol destination.")]
    InvalidProtocolDestination,
    #[msg("Pixel is locked.")]
    PixelLocked,
    #[msg("You already own this pixel.")]
    AlreadyOwner,
    #[msg("Math overflow.")]
    MathOverflow,
    #[msg("Pixel is already locked.")]
    PixelAlreadyLocked,
    #[msg("Unauthorized.")]
    Unauthorized,
    #[msg("Invalid BLOCK mint.")]
    InvalidBlockMint,
    #[msg("Invalid BLOCK token owner.")]
    InvalidBlockTokenOwner,
    #[msg("Insufficient BLOCK balance.")]
    InsufficientBlockBalance,
    #[msg("Invalid NFT mint.")]
    InvalidNftMint,
    #[msg("Invalid NFT token account owner.")]
    InvalidNftTokenOwner,
    #[msg("Pixel coordinates do not match the PDA account.")]
    PixelCoordinateMismatch,
    #[msg("Missing seller NFT delegate approval for billboard PDA.")]
    MissingNftDelegateApproval,
    #[msg("Invalid NFT freeze authority.")]
    InvalidNftFreezeAuthority,
}
