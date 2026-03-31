use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, Token, TokenAccount, Transfer};

declare_id!("2bizvF84cNRhEQdZPH7nBHCyA712uArhNe11CTg43Cm1");

const INITIAL_PRICE_MICRO_USDC: u64 = 1_000_000;
const MAX_NAME_LEN: usize = 32;
const MAX_DESCRIPTION_LEN: usize = 200;
const MAX_URL_LEN: usize = 200;
const MAX_IMAGE_DATA_LEN: usize = 1024; // temporaire pour Playground/dev
const LOCK_AMOUNT_BLOCK: u64 = 1_000;

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

    pub fn rebuy_pixel(
        ctx: Context<RebuyPixel>,
        new_name: String,
        new_description: String,
        new_image_data: Vec<u8>,
        new_url: String,
    ) -> Result<()> {
        validate_metadata(&new_name, &new_description, &new_image_data, &new_url)?;

        let pixel = &mut ctx.accounts.pixel;
        let billboard = &mut ctx.accounts.billboard;
        let signer = &ctx.accounts.signer;

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
            billboard.wallet_rebuy_fees,
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

        let transfer_to_seller_accounts = Transfer {
            from: ctx.accounts.buyer_usdc.to_account_info(),
            to: ctx.accounts.seller_usdc.to_account_info(),
            authority: signer.to_account_info(),
        };

        let transfer_to_seller_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            transfer_to_seller_accounts,
        );

        token::transfer(transfer_to_seller_ctx, seller_amount)?;

        let transfer_to_protocol_accounts = Transfer {
            from: ctx.accounts.buyer_usdc.to_account_info(),
            to: ctx.accounts.protocol_usdc.to_account_info(),
            authority: signer.to_account_info(),
        };

        let transfer_to_protocol_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            transfer_to_protocol_accounts,
        );

        token::transfer(transfer_to_protocol_ctx, protocol_amount)?;

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

        billboard.total_usdc_revenue = billboard
            .total_usdc_revenue
            .checked_add(protocol_amount)
            .ok_or(ErrorCode::MathOverflow)?;

        Ok(())
    }

    pub fn lock_pixel(ctx: Context<LockPixel>) -> Result<()> {
        let pixel = &mut ctx.accounts.pixel;
        let billboard = &mut ctx.accounts.billboard;
        let owner = &ctx.accounts.owner;

        require!(!pixel.locked, ErrorCode::PixelAlreadyLocked);
        require_keys_eq!(pixel.owner, owner.key(), ErrorCode::Unauthorized);
        require_keys_eq!(
            billboard.block_token_mint,
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

        let burn_accounts = Burn {
            mint: ctx.accounts.block_token_mint.to_account_info(),
            from: ctx.accounts.owner_block_token.to_account_info(),
            authority: owner.to_account_info(),
        };

        let burn_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), burn_accounts);
        token::burn(burn_ctx, burn_amount_raw)?;

        pixel.locked = true;
        pixel.locked_at_block = Clock::get()?.slot;

        billboard.total_pixels_locked = billboard
            .total_pixels_locked
            .checked_add(1)
            .ok_or(ErrorCode::MathOverflow)?;
        billboard.total_block_burned = billboard
            .total_block_burned
            .checked_add(LOCK_AMOUNT_BLOCK)
            .ok_or(ErrorCode::MathOverflow)?;

        Ok(())
    }

    pub fn update_metadata(
        ctx: Context<UpdateMetadata>,
        name: String,
        description: String,
        image_data: Vec<u8>,
        url: String,
    ) -> Result<()> {
        validate_metadata(&name, &description, &image_data, &url)?;

        let pixel = &mut ctx.accounts.pixel;
        let owner = &ctx.accounts.owner;

        require_keys_eq!(pixel.owner, owner.key(), ErrorCode::Unauthorized);

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

#[derive(Accounts)]
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
            pixel.x.to_le_bytes().as_ref(),
            pixel.y.to_le_bytes().as_ref()
        ],
        bump = pixel.bump
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
    pub seller_usdc: Account<'info, TokenAccount>,

    #[account(mut)]
    pub protocol_usdc: Account<'info, TokenAccount>,

    pub usdc_mint: Account<'info, Mint>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
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
            pixel.x.to_le_bytes().as_ref(),
            pixel.y.to_le_bytes().as_ref()
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

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct UpdateMetadata<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        seeds = [
            b"pixel".as_ref(),
            pixel.x.to_le_bytes().as_ref(),
            pixel.y.to_le_bytes().as_ref()
        ],
        bump = pixel.bump
    )]
    pub pixel: Account<'info, PixelAccount>,
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
}
