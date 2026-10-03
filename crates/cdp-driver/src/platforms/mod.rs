//! Platform connection configs and primitives shared by Kuaishou surfaces.
//!
//! Today this only hosts [`kuaishou`]: the Rust+chromiumoxide port of the
//! jieger Kuaishou platform layer. No browser launching, headless switching,
//! business scheduling, or account persistence lives here.

pub mod kuaishou;

pub use kuaishou::{
    build_startup_url, connect, connect_jinniu, connect_to, ensure_auth, ensure_auth_to,
    extract_jinniu_account_id_from_url, is_jinniu_login_page, is_kuaishou_login_page,
    is_sub_account_home_url, login, login_to, sub_account_live_room_url,
    wait_until_authenticated_url, AuthPhase, EnsureAuthOptions, EnsureAuthResult, KuaishouConfig,
    KuaishouJinniuConfig, KuaishouSubAccountConfig, KuaishouVerify, PhaseCallback,
    KS_CONNECT_TIMEOUT, KS_LOGIN_TIMEOUT, KUAISHOU_CONFIG, KUAISHOU_JINNIU_CONFIG,
    KUAISHOU_SUB_ACCOUNT_CONFIG,
};
