use crate::client::ServerEndpointInfo;
use std::sync::Arc;
use std::time::Duration;

use super::AuthInfo;
use crate::client::nacos_client::{ActixSystemCmd, ActixSystemResult};
use crate::init_global_system_actor;
use actix::{prelude::*, Context};

pub struct AuthActor {
    endpoints: Arc<ServerEndpointInfo>,
    auth: Option<AuthInfo>,
    client: reqwest::Client,
    use_auth: bool,
    token: Arc<String>,
    token_time_out: u64,
}

impl AuthActor {
    pub fn new(endpoints: Arc<ServerEndpointInfo>, auth_info: Option<AuthInfo>) -> Self {
        let use_auth = if let Some(auth) = &auth_info {
            auth.is_valid()
        } else {
            false
        };
        Self {
            endpoints,
            auth: auth_info,
            client: reqwest::Client::new(),
            use_auth,
            token: Default::default(),
            token_time_out: Default::default(),
        }
    }

    fn update_token(&mut self, ctx: &mut Context<Self>) {
        if !self.use_auth {
            return;
        }
        let client = self.client.clone();
        let endpoints = self.endpoints.clone();
        let auth = self.auth.clone();
        async move {
            let auth = auth.unwrap();
            super::Client::login(&client, endpoints, &auth).await
        }
        .into_actor(self)
        .map(|result, this, _| {
            if let Ok(token_info) = result {
                this.token = Arc::new(token_info.access_token);
                this.token_time_out = super::now_millis() + (token_info.token_ttl - 5) * 1000;
            }
        })
        .wait(ctx);
    }

    fn get_token(&mut self, ctx: &mut Context<Self>) -> Arc<String> {
        if !self.use_auth {
            return Default::default();
        }
        let now = super::now_millis();
        if now < self.token_time_out {
            return self.token.clone();
        }
        self.update_token(ctx);
        if self.token.is_empty() {
            log::warn!("get token is empty");
        }
        self.token.clone()
    }

    pub fn hb(&mut self, ctx: &mut Context<Self>) {
        if !self.use_auth {
            return;
        }
        let now = super::now_millis();
        if now + 60 * 1000 > self.token_time_out {
            self.update_token(ctx);
        }
        ctx.run_later(Duration::from_secs(30), |act, ctx| {
            act.hb(ctx);
        });
    }

    pub fn init_auth_actor(
        endpoints: Arc<ServerEndpointInfo>,
        auth_info: Option<AuthInfo>,
    ) -> Addr<AuthActor> {
        let system_addr = init_global_system_actor();
        let actor = AuthActor::new(endpoints, auth_info);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let msg = ActixSystemCmd::AuthActor(actor, tx);
        system_addr.do_send(msg);
        match rx.recv().unwrap() {
            ActixSystemResult::AuthActorAddr(auth_addr) => auth_addr,
            _ => panic!("init actor error"),
        }
    }
}

impl Actor for AuthActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("AuthActor started");
        self.hb(ctx);
        //ctx.run_later(Duration::from_nanos(1), |act,ctx|{
        //    act.hb(ctx);
        //});
    }
}

//type AuthHandleResultSender = tokio::sync::oneshot::Sender<AuthHandleResult>;

#[derive(Message)]
#[rtype(result = "Result<AuthHandleResult,std::io::Error>")]
pub enum AuthCmd {
    //QueryToken(AuthHandleResultSender),
    QueryToken,
}

pub enum AuthHandleResult {
    None,
    Token(Arc<String>),
}

impl Handler<AuthCmd> for AuthActor {
    type Result = Result<AuthHandleResult, std::io::Error>;
    fn handle(&mut self, msg: AuthCmd, ctx: &mut Context<Self>) -> Self::Result {
        match msg {
            AuthCmd::QueryToken => {
                let token = self.get_token(ctx);
                Ok(AuthHandleResult::Token(token))
            }
        }
    }
}

pub async fn get_token_result(auth_addr: &Addr<AuthActor>) -> anyhow::Result<Arc<String>> {
    match auth_addr.send(AuthCmd::QueryToken).await?? {
        AuthHandleResult::None => {}
        AuthHandleResult::Token(v) => {
            if v.len() > 0 {
                return Ok(v);
            }
        }
    };
    Ok(Default::default())
}
