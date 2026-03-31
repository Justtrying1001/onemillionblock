use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("2bizvF84cNRhEQdZPH7nBHCyA712uArhNe11CTg43Cm1");

const INITIAL_PRICE_MICRO_USDC: u64 = 1_000_000;
const MAX_NAME_LEN: usize = 32;
const MAX_DESCRIPTION_LEN: usize = 200;
const MAX_URL_LEN: usize = 200;
const MAX_IMAGE_DATA_LEN: usize = 1024; // temporaire pour Playground/dev

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

        let transfer_accounts = Transfer {
            from: ctx.accounts.buyer_usdc.to_account_info(),
            to: ctx.accounts.usdc_destination.to_account_info(),
            authority: ctx.accounts.signer.to_account_info(),
        };

        let transfer_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            transfer_accounts,
        );

        token::transfer(transfer_ctx, INITIAL_PRICE_MICRO_USDC)?;

        let pixel = &mut ctx.accounts.pixel;
        let billboard = &mut ctx.accounts.billboard;
        let signer = &ctx.accounts.signer;

        pixel.x = x;
        pixel.y = y;
        pixel.owner = signer.key();
        pixel.current_price = INITIAL_PRICE_MICRO_USDC;
        pixel.rebuy_count = 0;
        pixel.locked = false;
        pixel.locked_at_block = 0;
        pixel.nft_mint = Pubkey::default();

        pixel.name = name;
        pixel.description = description;
        pixel.image_data = image_data;
        pixel.url = url;

        pixel.bump = ctx.bumps.pixel;

        billboard.total_pixels_sold = billboard
            .total_pixels_sold
            .checked_add(1)
            .ok_or(ErrorCode::MathOverflow)?;

        billboard.total_usdc_revenue = billboard
            .total_usdc_revenue
            .checked_add(INITIAL_PRICE_MICRO_USDC)
            .ok_or(ErrorCode::MathOverflow)?;

        Ok(())
    }
}

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

    #[account(
        mut,
        token::authority = signer
    )]
    pub buyer_usdc: Account<'info, TokenAccount>,

    #[account(mut)]
    pub usdc_destination: Account<'info, TokenAccount>,

    pub usdc_mint: Account<'info, Mint>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

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
    pub const LEN: usize =
        2 +
        2 +
        32 +
        8 +
        1 +
        1 +
        8 +
        32 +
        4 + MAX_NAME_LEN +
        4 + MAX_DESCRIPTION_LEN +
        4 + MAX_IMAGE_DATA_LEN +
        4 + MAX_URL_LEN +
        1;
}

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
    #[msg("Math overflow.")]
    MathOverflow,
}