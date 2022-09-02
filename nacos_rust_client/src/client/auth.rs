

use std::time::Duration;
use std::sync::Arc;
use crate::client::ServerEndpointInfo;

use super::AuthInfo;
use actix::{prelude::*, Context};

pub struct AuthActor {
    endpoints:Arc<ServerEndpointInfo>,
    auth:Option<AuthInfo>,
    client: reqwest::Client,
    use_auth:bool,
    token: Arc<String>,
    token_time_out :u64,
}

impl AuthActor {

    pub fn new(endpoints:Arc<ServerEndpointInfo>,auth_info:Option<AuthInfo>) -> Self{
        let use_auth=
        if let Some(auth) = &auth_info {
            auth.is_valid()
        }
        else{
            false
        };
        Self{
            endpoints,
            auth :auth_info,
            client: reqwest::Client::new(),
            use_auth,
            token:Default::default(),
            token_time_out:Default::default(),
        }
    }

    fn update_token(&mut self,ctx:&mut Context<Self>){
        if !self.use_auth {
            return;
        }
        let client = self.client.clone();
        let endpoints = self.endpoints.clone();
        let auth = self.auth.clone();
        async move{
            let auth = auth.unwrap();
            let result=super::Client::login(&client, endpoints, &auth).await;
            result
        }
        .into_actor(self).map(|result,this,_|{
            match result {
                Ok(token_info) => {
                    this.token=Arc::new(token_info.access_token);
                    this.token_time_out = super::now_millis()+(token_info.token_ttl-5)*1000;
                },
                Err(_) => { },
            }
        }).wait(ctx);
    }

    fn get_token(&mut self,ctx:&mut Context<Self>) -> Arc<String>{
        if !self.use_auth{
            return Default::default();
        }
        let now = super::now_millis();
        if now < self.token_time_out {
            return self.token.clone();
        } 
        self.update_token(ctx);
        return self.token.clone();
    }
    
    pub fn hb(&mut self,ctx:&mut Context<Self>){
        if !self.use_auth {
            return;
        }
        let now = super::now_millis();
        if now + 60*1000 > self.token_time_out {
            self.update_token(ctx);
        } 
        ctx.run_later(Duration::from_secs(30), |act,ctx|{
            act.hb(ctx);
        });
    }
}

impl Actor for AuthActor {
    type Context = Context<Self>;

    fn started(&mut self,ctx:&mut Self::Context){
        log::info!("AuthActor started");
        self.hb(ctx);
        //ctx.run_later(Duration::from_nanos(1), |act,ctx|{
        //    act.hb(ctx);
        //});
    }
}

//type AuthHandleResultSender = tokio::sync::oneshot::Sender<AuthHandleResult>;

#[derive(Message)]
#[rtype(result="Result<AuthHandleResult,std::io::Error>")]
pub enum AuthCmd {
    //QueryToken(AuthHandleResultSender),
    QueryToken,
}

pub enum AuthHandleResult {
    None,
    Token(Arc<String>),
}

impl Handler<AuthCmd> for AuthActor {
    type Result = Result<AuthHandleResult,std::io::Error>;
    fn handle(&mut self,msg:AuthCmd,ctx:&mut Context<Self>) -> Self::Result {
        match msg {
            AuthCmd::QueryToken => {
                let token=self.get_token(ctx);
                Ok(AuthHandleResult::Token(token))
            },
        }
    }
}
