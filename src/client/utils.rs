use std::borrow::Cow;
use std::time::Duration;
use std::{collections::HashMap, str::FromStr};

use hyper::{Body, Client, Request, Response, body::Buf, client::HttpConnector, header::HeaderName};

pub fn ms(millis:u64) -> Duration {
    Duration::from_millis(millis)
}

pub struct Utils;

#[derive(Default,Clone,Debug)]
pub struct ResponseWrap{
    pub status:u16,
    pub headers:Vec<(String,String)>,
    pub body:Vec<u8>,
}

impl ResponseWrap {
    pub fn status_is_200(&self) -> bool {
        self.status==200
    }

    pub fn get_lossy_string_body(&self) -> Cow<str>{
        String::from_utf8_lossy(&self.body)
    }

    pub fn get_string_body(&self) -> String {
        String::from_utf8(self.body.clone()).unwrap()
    }

    pub fn get_map_headers(&self) -> HashMap<String,String> {
        Self::convert_to_map_headers(self.headers.clone())
    }

    pub fn convert_to_map_headers(headers:Vec<(String,String)>) -> HashMap<String,String> {
        let mut h = HashMap::new();
        for (k,v) in headers{
            h.insert(k, v);
        }
        h
    }
}


impl Utils {

    async fn get_response_wrap(resp:Response<Body>) -> anyhow::Result<ResponseWrap> {
        let status = resp.status().as_u16();
        let mut resp_headers = vec![];
        for (k,v) in resp.headers(){
            let value = String::from_utf8(v.as_bytes().to_vec())?;
            resp_headers.push((k.as_str().to_owned(),value));
        }
        let body=hyper::body::aggregate(resp).await?;
        let body= body.chunk().to_vec();
        Ok(ResponseWrap{
            status,
            headers:resp_headers,
            body:body
        })
    }

    pub async fn request(client:&Client<HttpConnector>,method_name:&str,url:&str,body:Vec<u8>,headers:Option<&HashMap<String,String>>,timeout_millis:Option<u64>) -> anyhow::Result<ResponseWrap> {
        let mut req = Request::new(Body::from(body));
        *req.uri_mut() = url.parse()?;
        *req.method_mut() = method_name.parse()?;
        if let Some(headers) = headers {
            for (key,val) in headers.iter() {
                let a = val as &str;
                req.headers_mut().insert(HeaderName::from_str(key)?, a.parse()?);
            }
        }
        if let Some(timeout_millis) = timeout_millis {
            tokio::select! {
                Ok(resp)= client.request(req) => {
                    Self::get_response_wrap(resp).await
                },
                _ = tokio::time::sleep(ms(timeout_millis)) => {
                    Err(anyhow::anyhow!("request time out {}",&timeout_millis))
                },
            }
        }
        else{
            let resp=client.request(req).await?;
            Self::get_response_wrap(resp).await
        }
    }
}