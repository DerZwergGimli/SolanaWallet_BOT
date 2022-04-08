use std::collections::HashSet;
use std::env;
use std::sync::Arc;
use std::time::Duration;

use env_logger;
use log::{error, info};
use serenity::{
    async_trait,
    client::bridge::gateway::ShardManager,
    framework::StandardFramework,
    http::Http,
    model::{event::ResumedEvent, gateway::Ready},
    prelude::*,
};
use serenity::model::id::{GuildId, UserId};
use serenity::model::prelude::RoleId;
use solanabeach::*;

// A container type is created for inserting into the Client's `data`, which
// allows for data to be accessible across all events and framework commands, or
// anywhere else that has a copy of the `data` Arc.
// These places are usually where either Context or Client is present.
//
// Documentation about TypeMap can be found here:
// https://docs.rs/typemap_rev/0.1/typemap_rev/struct.TypeMap.html


pub struct ShardManagerContainer;

impl TypeMapKey for ShardManagerContainer {
    type Value = Arc<Mutex<ShardManager>>;
}

struct Handler;


#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        info!("Connected as {}", ready.user.name);
    }

    async fn resume(&self, _: Context, _: ResumedEvent) {
        info!("Resumed");
    }

    async fn cache_ready(&self, ctx: Context, guilds: Vec<GuildId>) {
        let sleep_time = env::var("LOOP_SLEEP").unwrap_or("0".to_string()).to_string().parse::<u64>().unwrap();
        let user_ids = env::var("USER_ID").expect("ENV USER_ID Not-Found")
            .split(",")
            .map(|a| a.to_string())
            .collect::<Vec<_>>();
        let color_threshold = env::var("COLOR_THRESHOLD").expect("ENV COLOR_THRESHOLD Not-Found").parse::<i32>().unwrap();

                if sleep_time != 0 {
            info!("Starting update loop...");
            loop {
                if env::var("UPDATE_NAME").unwrap_or("true".to_string()).parse::<bool>().unwrap() {
                    info!("Message posted");
                    for guild in guilds.clone() {

                        //Get Solana TPS
                        let blocks = solana_beach_api::latest_blocks(50).await.unwrap();
                        let timespan = blocks.first().unwrap().block_time.absolute - blocks.last().unwrap().block_time.absolute;


                        let mut sum_tx = 0;
                        for block in blocks {
                            sum_tx = sum_tx + block.metrics.tx_count;
                        }
                        let tps = sum_tx / timespan;
                        match tps {
                            tps if tps > color_threshold => {
                                guild.edit_nickname(&ctx, Some(format!("🚀  ~{} TPS", tps).as_ref())).await;
                            }
                            tps if tps < color_threshold => {
                                guild.edit_nickname(&ctx, Some(format!("🔥  ~{} TPS", tps).as_ref())).await;
                            }
                            _ => {}
                        };

                        //Get Role ids form Name
                        let mut red_role_id: RoleId = Default::default();
                        let mut green_role_id: RoleId = Default::default();
                        for guild in guilds.clone() {
                            let roles = guild.roles(&ctx).await;
                            for (_role_id, role) in roles.unwrap() {
                                if role.name.contains("tickers-red") {
                                    red_role_id = role.id;
                                }
                                if role.name.contains("tickers-green") {
                                    green_role_id = role.id;
                                }
                            }
                        }

                        for user_id in user_ids.clone() {
                            let uid = user_id.to_string().parse::<u64>().unwrap();

                            match tps {
                                tps if tps > color_threshold => {
                                    guild.member(&ctx.http, UserId(uid)).await.unwrap().remove_role(&ctx, red_role_id).await.unwrap();
                                    guild.member(&ctx.http, UserId(uid)).await.unwrap().add_role(&ctx, green_role_id).await.unwrap();
                                }
                                tps if tps < color_threshold => {
                                    guild.member(&ctx.http, UserId(uid)).await.unwrap().remove_role(&ctx, green_role_id).await.unwrap();
                                    guild.member(&ctx.http, UserId(uid)).await.unwrap().add_role(&ctx, red_role_id).await.unwrap();
                                }
                                _ => {}
                            };
                        }
                    }
                }
                tokio::time::sleep(Duration::from_secs(sleep_time)).await;
            }
        }
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();
    //tracing_subscriber::fmt::init();

    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");
    let http = Http::new_with_token(&token);

    // We will fetch your bot's owners and id
    let (owners, _bot_id) = match http.get_current_application_info().await {
        Ok(info) => {
            let mut owners = HashSet::new();
            owners.insert(info.owner.id);

            (owners, info.id)
        }
        Err(why) => panic!("Could not access application info: {:?}", why),
    };

    // Create the framework
    let framework =
        StandardFramework::new().configure(|c| c.owners(owners).prefix("~"));


    let mut client = Client::builder(&token)
        .framework(framework)
        .event_handler(Handler)
        .await
        .expect("Err creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<ShardManagerContainer>(client.shard_manager.clone());
    }

    let shard_manager = client.shard_manager.clone();

    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.expect("Could not register ctrl+c handler");
        shard_manager.lock().await.shutdown_all().await;
    });

    if let Err(why) = client.start().await {
        error!("Client error: {:?}", why);
    }
}
