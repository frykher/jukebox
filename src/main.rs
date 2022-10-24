use std::collections::HashMap;
use std::sync::Arc;

use futures_util::stream::SplitStream;
use futures_util::StreamExt;
use jukebox::utils::handle_message;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio_tungstenite::connect_async;
use warp::ws::{Message, WebSocket};
use warp::Filter;

use jukebox::client::payloads::{ClientPayload, Opcode, VoiceUpdate};
use jukebox::client::player::Player;
use jukebox::client::{Client, Headers};

use jukebox::discord::payloads::{DiscordPayload, Identify};

const PASSWORD: &str = "youshallnotpass";

#[derive(Debug)]
struct Unauthorized;

impl warp::reject::Reject for Unauthorized {}

// https://github.com/freyacodes/Lavalink/blob/master/IMPLEMENTATION.md

#[tokio::main]
async fn main() {
    let headers = warp::any()
        .and(warp::header::<String>("Authorization"))
        .and(warp::header::<String>("User-Id"))
        .and(warp::header::<String>("Client-Name"))
        .and_then(|authorization, user_id, client_name| async move {
            if let Some(headers) =
                Headers::new(authorization, user_id, client_name).verify(PASSWORD)
            {
                Ok(headers)
            } else {
                Err(warp::reject::custom(Unauthorized))
            }
        });

    // ws://127.0.0.1/
    let gateway = warp::get()
        .and(headers)
        .and(warp::ws())
        .map(|headers, ws: warp::ws::Ws| {
            ws.on_upgrade(move |websocket| handle_websocket(websocket, headers))
        });

    // GET /loadtracks?identifier=dQw4w9WgXcQ
    let loadtracks = warp::path!("loadtracks")
        .and(warp::query::<HashMap<String, String>>())
        .map(
            |params: HashMap<String, String>| match params.get("identifier") {
                Some(identifier) => format!("Loading tracks with identifier {}", identifier),
                None => "No identifier provided".to_string(),
            },
        );

    // GET /decodetrack?track=<trackid>
    let decodetrack = warp::path!("decodetrack")
        .and(warp::query::<HashMap<String, String>>())
        .map(
            |params: HashMap<String, String>| match params.get("track") {
                Some(track) => format!("Decoding track {}", track),
                None => "No track provided".to_string(),
            },
        );

    // POST /decodetracks
    let decodetracks = warp::path!("decodetracks")
        .and(warp::body::json())
        .map(|tracks: Vec<String>| format!("Decoding tracks {:?}", tracks));

    let routes = gateway
        .or(loadtracks)
        .or(decodetrack)
        .or(decodetracks)
        .recover(|err: warp::Rejection| async move {
            if let Some(Unauthorized) = err.find() {
                Ok::<_, warp::Rejection>(warp::reply::with_status(
                    "Unauthorized",
                    warp::http::StatusCode::UNAUTHORIZED,
                ))
            } else {
                Err(err)
            }
        });

    warp::serve(routes).run(([127, 0, 0, 1], 3030)).await;
}

// first payload should be voiceUpdate
async fn handle_websocket(websocket: warp::ws::WebSocket, headers: Headers) {
    let (tx, mut rx) = websocket.split();
    let client = Arc::new(Client::new(headers, tx));

    let (player_tx, mut player_rx) = unbounded_channel::<String>();

    let weak_client = Arc::downgrade(&client);
    tokio::spawn(async move {
        while let Some(msg) = player_rx.recv().await {
            eprintln!("{:?}", msg);
            if let Some(client) = weak_client.upgrade() {
                client.send(Message::text(msg)).await;
            }
        }
    });

    while let Some(payload) =
        // jesus
        handle_message::<_, _, _, ClientPayload>(&mut rx).await
    {
        match payload.op {
            Opcode::VoiceUpdate(voice_update) => {
                tokio::spawn(create_player(client.clone(), player_tx.clone(), voice_update))
            }
            _ => {
                println!("recieved");
                match client.get_player_sender(&payload.guild_id).await {
                    // receiver will never be dropped so long as player is alive
                    Some(sender) => sender.send(payload).unwrap(),
                    None => {
                        client
                            .send(Message::text("No player associated with this guild_id"))
                            .await
                    }
                }
            }
        }
    }
}

async fn create_player(
    client: Arc<Client>,
    player_tx: UnboundedSender<String>,
    voice_update: VoiceUpdate,
) {
    let mut player = match Player::new(client.id(), voice_update, player_tx).await {
        Ok(player) => player,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };
    if let Err(e) = player.start().await {
        eprintln!("Player Error:, {}", e);
        return;
    }
    client.add_player(player).await;

    let example1 = vec!["Priority 2", "Priority 1", "Priority 3"];
    // should be Priority 1
    let result: String = example1.into_iter().easy_func(/*stuff */);

    let example3 = vec!["Priority 3", "Priority 2"];
    // result is Priority 2
    let result: String = example1.into_iter().easy_func(/*stuff */);

    let example4 = vec!["Priority 3"];
    // result is Priority 3
    let result: String = example1.into_iter().easy_func(/*stuff */);
}

fn handle_payload(payload: ClientPayload) {
    match payload.op {
        Opcode::VoiceUpdate(voice_update) => {
            println!("VoiceUpdate: {:?}", voice_update);
        }
        Opcode::Play(play) => {
            println!("Play: {:?}", play);
        }
        Opcode::Stop(stop) => {
            println!("Stop: {:?}", stop);
        }
        Opcode::Pause(pause) => {
            println!("Pause: {:?}", pause);
        }
        Opcode::Seek(seek) => {
            println!("Seek: {:?}", seek);
        }
        Opcode::Volume(volume) => {
            println!("Volume: {:?}", volume);
        }
        Opcode::Filters(filters) => {
            println!("Filters: {:?}", filters);
        }
        Opcode::Destroy(destroy) => {
            println!("Destroy: {:?}", destroy);
        }
    }
}
