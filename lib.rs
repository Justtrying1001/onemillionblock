use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, Token, TokenAccount, Transfer};

declare_id!("2bizvF84cNRhEQdZPH7nBHCyA712uArhNe11CTg43Cm1");

const INITIAL_PRICE_MICRO_USDC: u64 = 1_000_000;
const LOCK_AMOUNT_BLOCK: u64 = 1_000;
const MAX_NAME_LEN: usize = 32;
const MAX_DESCRIPTION_LEN: usize = 200;
const MAX_URL_LEN: usize = 200;

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

    pub fn create_content(
        ctx: Context<CreateContent>,
        name: String,
        description: String,
        url: String,
    ) -> Result<()> {
        validate_content_fields(&name, &description, &url)?;

        let content = &mut ctx.accounts.content;
        content.authority = ctx.accounts.authority.key();
        content.name = name;
        content.description = description;
        content.url = url;
        Ok(())
    }

    pub fn update_content(
        ctx: Context<UpdateContent>,
        name: String,
        description: String,
        url: String,
    ) -> Result<()> {
        validate_content_fields(&name, &description, &url)?;

        let content = &mut ctx.accounts.content;
        require_keys_eq!(
            content.authority,
            ctx.accounts.authority.key(),
            ErrorCode::Unauthorized
        );
        content.name = name;
        content.description = description;
        content.url = url;
        Ok(())
    }

    pub fn buy_pixel(
        ctx: Context<BuyPixel>,
        x: u16,
        y: u16,
        color: u32,
        content_ref: Option<Pubkey>,
    ) -> Result<()> {
        require!(x < 1000, ErrorCode::InvalidCoordinate);
        require!(y < 1000, ErrorCode::InvalidCoordinate);

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
        validate_content_ref(
            content_ref,
            ctx.accounts.content.as_ref(),
            ctx.accounts.signer.key(),
        )?;

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

        let pixel = &mut ctx.accounts.pixel;
        pixel.x = x;
        pixel.y = y;
        pixel.owner = ctx.accounts.signer.key();
        pixel.current_price = INITIAL_PRICE_MICRO_USDC;
        pixel.rebuy_count = 0;
        pixel.locked = false;
        pixel.locked_at_slot = 0;
        pixel.color = color;
        pixel.content_ref = content_ref;
        pixel.bump = ctx.bumps.pixel;

        let billboard = &mut ctx.accounts.billboard;
        billboard.total_pixels_sold = billboard
            .total_pixels_sold
            .checked_add(1)
            .ok_or(ErrorCode::MathOverflow)?;

        Ok(())
    }

    pub fn rebuy_pixel(
        ctx: Context<RebuyPixel>,
        x: u16,
        y: u16,
        new_color: u32,
        new_content_ref: Option<Pubkey>,
    ) -> Result<()> {
        require!(x < 1000, ErrorCode::InvalidCoordinate);
        require!(y < 1000, ErrorCode::InvalidCoordinate);

        let pixel = &mut ctx.accounts.pixel;
        let buyer = &ctx.accounts.signer;

        require!(pixel.x == x, ErrorCode::PixelCoordinateMismatch);
        require!(pixel.y == y, ErrorCode::PixelCoordinateMismatch);
        require!(!pixel.locked, ErrorCode::PixelLocked);
        require!(pixel.owner != buyer.key(), ErrorCode::AlreadyOwner);

        require_keys_eq!(
            ctx.accounts.buyer_usdc.owner,
            buyer.key(),
            ErrorCode::InvalidBuyerTokenOwner
        );
        require_keys_eq!(
            ctx.accounts.seller_usdc.owner,
            pixel.owner,
            ErrorCode::InvalidSellerTokenOwner
        );
        require_keys_eq!(
            ctx.accounts.protocol_usdc.owner,
            ctx.accounts.billboard.wallet_rebuy_fees,
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
        validate_content_ref(new_content_ref, ctx.accounts.content.as_ref(), buyer.key())?;

        let new_price = pixel
            .current_price
            .checked_mul(2)
            .ok_or(ErrorCode::MathOverflow)?;
        let seller_amount = new_price
            .checked_mul(95)
            .ok_or(ErrorCode::MathOverflow)?
            .checked_div(100)
            .ok_or(ErrorCode::MathOverflow)?;
        let protocol_fee = new_price
            .checked_sub(seller_amount)
            .ok_or(ErrorCode::MathOverflow)?;

        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.buyer_usdc.to_account_info(),
                    to: ctx.accounts.seller_usdc.to_account_info(),
                    authority: buyer.to_account_info(),
                },
            ),
            seller_amount,
        )?;

        token::transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.buyer_usdc.to_account_info(),
                    to: ctx.accounts.protocol_usdc.to_account_info(),
                    authority: buyer.to_account_info(),
                },
            ),
            protocol_fee,
        )?;

        pixel.owner = buyer.key();
        pixel.current_price = new_price;
        pixel.rebuy_count = pixel
            .rebuy_count
            .checked_add(1)
            .ok_or(ErrorCode::MathOverflow)?;
        pixel.color = new_color;
        pixel.content_ref = new_content_ref;

        let billboard = &mut ctx.accounts.billboard;
        billboard.total_usdc_revenue = billboard
            .total_usdc_revenue
            .checked_add(protocol_fee)
            .ok_or(ErrorCode::MathOverflow)?;

        Ok(())
    }

    pub fn lock_pixel(ctx: Context<LockPixel>, x: u16, y: u16) -> Result<()> {
        require!(x < 1000, ErrorCode::InvalidCoordinate);
        require!(y < 1000, ErrorCode::InvalidCoordinate);

        let pixel = &mut ctx.accounts.pixel;
        let owner = &ctx.accounts.owner;

        require!(pixel.x == x, ErrorCode::PixelCoordinateMismatch);
        require!(pixel.y == y, ErrorCode::PixelCoordinateMismatch);
        require!(!pixel.locked, ErrorCode::PixelAlreadyLocked);
        require_keys_eq!(pixel.owner, owner.key(), ErrorCode::Unauthorized);
        require_keys_eq!(
            ctx.accounts.block_token_mint.key(),
            ctx.accounts.billboard.block_token_mint,
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

        pixel.locked = true;
        pixel.locked_at_slot = Clock::get()?.slot;

        let billboard = &mut ctx.accounts.billboard;
        billboard.total_pixels_locked = billboard
            .total_pixels_locked
            .checked_add(1)
            .ok_or(ErrorCode::MathOverflow)?;
        billboard.total_block_burned = billboard
            .total_block_burned
            .checked_add(burn_amount_raw)
            .ok_or(ErrorCode::MathOverflow)?;

        Ok(())
    }

    pub fn update_pixel(
        ctx: Context<UpdatePixel>,
        x: u16,
        y: u16,
        color: u32,
        content_ref: Option<Pubkey>,
    ) -> Result<()> {
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
        validate_content_ref(
            content_ref,
            ctx.accounts.content.as_ref(),
            ctx.accounts.owner.key(),
        )?;

        pixel.color = color;
        pixel.content_ref = content_ref;

        Ok(())
    }
}

fn validate_content_fields(name: &str, description: &str, url: &str) -> Result<()> {
    require!(name.len() <= MAX_NAME_LEN, ErrorCode::NameTooLong);
    require!(
        description.len() <= MAX_DESCRIPTION_LEN,
        ErrorCode::DescriptionTooLong
    );
    require!(url.len() <= MAX_URL_LEN, ErrorCode::UrlTooLong);
    Ok(())
}

fn validate_content_ref(
    content_ref: Option<Pubkey>,
    content: Option<&Account<ContentAccount>>,
    signer: Pubkey,
) -> Result<()> {
    match (content_ref, content) {
        (Some(content_ref_key), Some(content_account)) => {
            require_keys_eq!(
                content_account.key(),
                content_ref_key,
                ErrorCode::ContentRefMismatch
            );
            require_keys_eq!(
                content_account.authority,
                signer,
                ErrorCode::UnauthorizedContentAuthority
            );
        }
        (Some(_), None) => return err!(ErrorCode::MissingContentAccount),
        (None, Some(_)) => return err!(ErrorCode::UnexpectedContentAccount),
        (None, None) => {}
    }
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
pub struct CreateContent<'info> {
    #[account(
        init,
        payer = authority,
        space = 8 + ContentAccount::LEN
    )]
    pub content: Account<'info, ContentAccount>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct UpdateContent<'info> {
    #[account(mut)]
    pub content: Account<'info, ContentAccount>,

    #[account(mut)]
    pub authority: Signer<'info>,
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
        seeds = [b"pixel", x.to_le_bytes().as_ref(), y.to_le_bytes().as_ref()],
        bump
    )]
    pub pixel: Account<'info, PixelAccount>,

    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(mut, token::authority = signer)]
    pub buyer_usdc: Account<'info, TokenAccount>,

    #[account(mut)]
    pub usdc_destination: Account<'info, TokenAccount>,

    pub usdc_mint: Account<'info, Mint>,

    pub content: Option<Account<'info, ContentAccount>>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
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
        seeds = [b"pixel", x.to_le_bytes().as_ref(), y.to_le_bytes().as_ref()],
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

    pub content: Option<Account<'info, ContentAccount>>,

    pub token_program: Program<'info, Token>,
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
        seeds = [b"pixel", x.to_le_bytes().as_ref(), y.to_le_bytes().as_ref()],
        bump = pixel.bump
    )]
    pub pixel: Account<'info, PixelAccount>,

    pub block_token_mint: Account<'info, Mint>,

    #[account(mut)]
    pub owner_block_token: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(x: u16, y: u16)]
pub struct UpdatePixel<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        mut,
        seeds = [b"pixel", x.to_le_bytes().as_ref(), y.to_le_bytes().as_ref()],
        bump = pixel.bump
    )]
    pub pixel: Account<'info, PixelAccount>,

    pub content: Option<Account<'info, ContentAccount>>,
}

#[account]
pub struct BillboardAccount {
    pub total_pixels_sold: u32,
    pub total_pixels_locked: u32,
    /// Total amount of BLOCK burned in raw mint units (base units),
    /// not human units. Frontend should divide by 10^decimals.
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
    pub locked_at_slot: u64,
    pub color: u32,
    pub content_ref: Option<Pubkey>,
    pub bump: u8,
}

impl PixelAccount {
    pub const LEN: usize = 2 + 2 + 32 + 8 + 1 + 1 + 8 + 4 + 1 + 32 + 1;
}

#[account]
pub struct ContentAccount {
    pub authority: Pubkey,
    pub name: String,
    pub description: String,
    pub url: String,
}

impl ContentAccount {
    pub const LEN: usize = 32 + 4 + MAX_NAME_LEN + 4 + MAX_DESCRIPTION_LEN + 4 + MAX_URL_LEN;
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
    #[msg("Pixel coordinates do not match the PDA account.")]
    PixelCoordinateMismatch,
    #[msg("Missing content account for content_ref.")]
    MissingContentAccount,
    #[msg("content_ref does not match provided content account.")]
    ContentRefMismatch,
    #[msg("Signer is not authorized to use this content.")]
    UnauthorizedContentAuthority,
    #[msg("Content account provided while content_ref is None.")]
    UnexpectedContentAccount,
}
