//! Tool catalog and dispatch for the Hotel MCP server.

use crate::state::{Booking, HousekeepingRequest, RoomOrder, SharedState, Ticket};
use serde_json::{json, Value};
use skill_hotel::HotelIntentKind;
use std::sync::atomic::{AtomicU64, Ordering};

static TICKET_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_ticket(prefix: &str) -> String {
    let n = TICKET_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{n:04}")
}

/// MCP tool descriptor returned by `tools/list`.
#[derive(serde::Serialize)]
pub struct ToolDescriptor {
    pub name: &'static str,
    pub description: &'static str,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}

/// Catalog used by `tools/list`.
pub fn catalog() -> Vec<ToolDescriptor> {
    HotelIntentKind::ALL
        .iter()
        .copied()
        .map(|kind| ToolDescriptor {
            name: kind.as_str(),
            description: description_for(kind),
            input_schema: schema_for(kind),
        })
        .collect()
}

/// Dispatch a `tools/call` to the matching handler. Returns the
/// MCP-style result envelope (`content`, `isError`, `structuredContent`).
pub async fn call(state: &SharedState, name: &str, args: &Value) -> Value {
    let Some(kind) = HotelIntentKind::parse(name) else {
        return error_envelope(&format!("unknown tool: {name}"));
    };
    let mut state = state.lock().await;
    match kind {
        HotelIntentKind::SetRoomTemperature => {
            let celsius = args.get("celsius").and_then(Value::as_f64).unwrap_or(21.0);
            state.room_temperature_celsius = Some(celsius);
            ok_envelope(
                format!("Room temperature set to {celsius:.1} °C."),
                json!({ "celsius": celsius }),
            )
        }
        HotelIntentKind::SetLights => {
            let scene = args
                .get("scene")
                .and_then(Value::as_str)
                .unwrap_or("default")
                .to_string();
            let brightness = args
                .get("brightness")
                .and_then(Value::as_u64)
                .map(|v| v.min(100) as u8);
            state.lights_scene = Some(scene.clone());
            state.lights_brightness = brightness;
            let brightness_text = brightness.map(|b| format!(" at {b}%")).unwrap_or_default();
            ok_envelope(
                format!("Lights set to scene '{scene}'{brightness_text}."),
                json!({ "scene": scene, "brightness": brightness }),
            )
        }
        HotelIntentKind::SetCurtains => {
            let action = args
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("open")
                .to_string();
            state.curtains_position = Some(action.clone());
            ok_envelope(format!("Curtains {action}."), json!({ "action": action }))
        }
        HotelIntentKind::SetTv => {
            let action = args
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("power_on")
                .to_string();
            state.tv_state = Some(action.clone());
            ok_envelope(format!("TV: {action}."), json!({ "action": action }))
        }
        HotelIntentKind::SetAmbientMusic => {
            let mood = args
                .get("mood")
                .and_then(Value::as_str)
                .unwrap_or("relax")
                .to_string();
            state.ambient_music = Some(mood.clone());
            ok_envelope(
                format!("Ambient music tuned to {mood}."),
                json!({ "mood": mood }),
            )
        }
        HotelIntentKind::SetDoNotDisturb => {
            let enabled = args.get("enabled").and_then(Value::as_bool).unwrap_or(true);
            let until = args
                .get("until")
                .and_then(Value::as_str)
                .map(str::to_string);
            state.do_not_disturb = enabled;
            state.do_not_disturb_until = until.clone();
            let summary = if enabled {
                match &until {
                    Some(t) => format!("Do-not-disturb enabled until {t}."),
                    None => "Do-not-disturb enabled.".to_string(),
                }
            } else {
                "Do-not-disturb disabled.".to_string()
            };
            ok_envelope(summary, json!({ "enabled": enabled, "until": until }))
        }
        HotelIntentKind::OrderRoomService => {
            let id = next_ticket("RS");
            let items = args.get("items").cloned().unwrap_or_else(|| json!([]));
            let eta = 25u32;
            state.orders.push(RoomOrder {
                id: id.clone(),
                kind: "room_service".to_string(),
                eta_minutes: eta,
                items: items.clone(),
            });
            ok_envelope(
                format!("Order {id} placed; estimated delivery in {eta} minutes."),
                json!({ "order_id": id, "eta_minutes": eta, "items": items }),
            )
        }
        HotelIntentKind::RequestHousekeeping => {
            let kind_str = args
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("daily")
                .to_string();
            let when = args.get("when").and_then(Value::as_str).map(str::to_string);
            let id = next_ticket("HK");
            state.housekeeping.push(HousekeepingRequest {
                id: id.clone(),
                kind: kind_str.clone(),
                when: when.clone(),
            });
            ok_envelope(
                format!("Housekeeping ({kind_str}) request {id} queued."),
                json!({ "request_id": id, "kind": kind_str, "when": when }),
            )
        }
        HotelIntentKind::RequestExtraTowels => {
            let count = args.get("count").and_then(Value::as_u64).unwrap_or(2) as u32;
            let id = next_ticket("TWL");
            state.housekeeping.push(HousekeepingRequest {
                id: id.clone(),
                kind: format!("extra_towels:{count}"),
                when: None,
            });
            ok_envelope(
                format!("{count} extra towels on the way (ticket {id})."),
                json!({ "request_id": id, "count": count }),
            )
        }
        HotelIntentKind::RequestExtraPillows => {
            let count = args.get("count").and_then(Value::as_u64).unwrap_or(1) as u32;
            let id = next_ticket("PIL");
            state.housekeeping.push(HousekeepingRequest {
                id: id.clone(),
                kind: format!("extra_pillows:{count}"),
                when: None,
            });
            ok_envelope(
                format!("{count} extra pillows on the way (ticket {id})."),
                json!({ "request_id": id, "count": count }),
            )
        }
        HotelIntentKind::RequestToiletries => {
            let items = args.get("items").cloned().unwrap_or_else(|| json!([]));
            let id = next_ticket("AMN");
            state.housekeeping.push(HousekeepingRequest {
                id: id.clone(),
                kind: "toiletries".to_string(),
                when: None,
            });
            ok_envelope(
                format!("Toiletries on the way (ticket {id})."),
                json!({ "request_id": id, "items": items }),
            )
        }
        HotelIntentKind::RequestLaundryPickup => {
            let when = args.get("when").and_then(Value::as_str).map(str::to_string);
            let id = next_ticket("LAU");
            state.housekeeping.push(HousekeepingRequest {
                id: id.clone(),
                kind: "laundry_pickup".to_string(),
                when: when.clone(),
            });
            let when_text = when
                .clone()
                .unwrap_or_else(|| "next available slot".to_string());
            ok_envelope(
                format!("Laundry pickup scheduled for {when_text} (ticket {id})."),
                json!({ "request_id": id, "when": when }),
            )
        }
        HotelIntentKind::RequestIron => {
            let kind_str = args
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("both")
                .to_string();
            let id = next_ticket("IRN");
            state.housekeeping.push(HousekeepingRequest {
                id: id.clone(),
                kind: format!("iron:{kind_str}"),
                when: None,
            });
            ok_envelope(
                format!("Iron request ({kind_str}) on the way (ticket {id})."),
                json!({ "request_id": id, "kind": kind_str }),
            )
        }
        HotelIntentKind::SetWakeUpCall => {
            let time = args
                .get("time")
                .and_then(Value::as_str)
                .unwrap_or("07:00")
                .to_string();
            state.wake_up_call_time = Some(time.clone());
            ok_envelope(
                format!("Wake-up call set for {time}."),
                json!({ "time": time }),
            )
        }
        HotelIntentKind::RequestLateCheckout => {
            let until = args
                .get("until")
                .and_then(Value::as_str)
                .unwrap_or("13:00")
                .to_string();
            state.late_checkout_until = Some(until.clone());
            ok_envelope(
                format!("Late checkout granted until {until}."),
                json!({ "until": until, "approved": true }),
            )
        }
        HotelIntentKind::BookRestaurant => {
            let outlet = args
                .get("outlet")
                .and_then(Value::as_str)
                .unwrap_or("main_dining")
                .to_string();
            let time = args
                .get("time")
                .and_then(Value::as_str)
                .unwrap_or("20:00")
                .to_string();
            let party_size = args.get("party_size").and_then(Value::as_u64).unwrap_or(2);
            let id = next_ticket("RES");
            state.bookings.push(Booking {
                id: id.clone(),
                kind: "restaurant".to_string(),
                time: Some(time.clone()),
                details: json!({ "outlet": outlet, "party_size": party_size }),
            });
            ok_envelope(
                format!(
                    "Restaurant booking confirmed at {outlet} for {party_size} at {time} (ref {id})."
                ),
                json!({
                    "booking_id": id,
                    "outlet": outlet,
                    "time": time,
                    "party_size": party_size,
                }),
            )
        }
        HotelIntentKind::BookSpa => {
            let service = args
                .get("service")
                .and_then(Value::as_str)
                .unwrap_or("massage")
                .to_string();
            let time = args
                .get("time")
                .and_then(Value::as_str)
                .unwrap_or("16:00")
                .to_string();
            let id = next_ticket("SPA");
            state.bookings.push(Booking {
                id: id.clone(),
                kind: "spa".to_string(),
                time: Some(time.clone()),
                details: json!({ "service": service }),
            });
            ok_envelope(
                format!("Spa ({service}) confirmed at {time} (ref {id})."),
                json!({ "booking_id": id, "service": service, "time": time }),
            )
        }
        HotelIntentKind::BookTaxi => {
            let destination = args
                .get("destination")
                .and_then(Value::as_str)
                .unwrap_or("front_door")
                .to_string();
            let time = args.get("time").and_then(Value::as_str).map(str::to_string);
            let passengers = args.get("passengers").and_then(Value::as_u64).unwrap_or(1);
            let id = next_ticket("TAX");
            state.bookings.push(Booking {
                id: id.clone(),
                kind: "taxi".to_string(),
                time: time.clone(),
                details: json!({ "destination": destination, "passengers": passengers }),
            });
            let time_text = time.clone().unwrap_or_else(|| "next available".to_string());
            ok_envelope(
                format!(
                    "Taxi to {destination} for {passengers} confirmed for {time_text} (ref {id})."
                ),
                json!({
                    "booking_id": id,
                    "destination": destination,
                    "time": time,
                    "passengers": passengers,
                }),
            )
        }
        HotelIntentKind::ConciergeInfo => {
            let topic = args
                .get("topic")
                .and_then(Value::as_str)
                .unwrap_or("general")
                .to_string();
            let answer = match topic.to_lowercase().as_str() {
                "pool" | "pool hours" => "Pool: 06:00 to 22:00, towels available poolside.",
                "gym" | "gym hours" => "Gym: 24/7 with key card access on level B1.",
                "wifi" | "wifi password" => "Wi-Fi network 'guest', password 'welcome2026'.",
                "breakfast" | "breakfast hours" => {
                    "Breakfast served 06:30 to 10:30 in the main dining room."
                }
                "checkout" | "checkout time" => {
                    "Standard checkout is 11:00. Late checkout can be requested."
                }
                "room temperature" | "temperature" => {
                    "Current room temperature setpoint can be changed via the room control."
                }
                _ => "I'll connect you to the concierge desk for that. Front desk extension 0.",
            };
            ok_envelope(answer, json!({ "topic": topic, "answer": answer }))
        }
        HotelIntentKind::RequestLocalRecommendation => {
            let topic = args
                .get("topic")
                .and_then(Value::as_str)
                .unwrap_or("things to do")
                .to_string();
            let suggestions = vec![
                json!({ "name": "Trattoria del Centro", "type": "italian", "walk_minutes": 6 }),
                json!({ "name": "Mercato Comunale", "type": "market", "walk_minutes": 9 }),
                json!({ "name": "Sky Lounge", "type": "rooftop_bar", "walk_minutes": 3 }),
            ];
            ok_envelope(
                format!("Top picks near the hotel for '{topic}'."),
                json!({ "topic": topic, "suggestions": suggestions }),
            )
        }
        HotelIntentKind::ReportComplaint => {
            let category = args
                .get("category")
                .and_then(Value::as_str)
                .unwrap_or("general")
                .to_string();
            let details = args
                .get("details")
                .and_then(Value::as_str)
                .map(str::to_string);
            let id = next_ticket("CMP");
            state.complaints.push(Ticket {
                id: id.clone(),
                category: category.clone(),
                details: details.clone(),
            });
            ok_envelope(
                format!("Complaint logged ({category}) — ticket {id}, manager will follow up."),
                json!({ "ticket_id": id, "category": category, "details": details }),
            )
        }
        HotelIntentKind::ReportLostItem => {
            let item = args
                .get("item")
                .and_then(Value::as_str)
                .unwrap_or("unspecified")
                .to_string();
            let last_seen = args
                .get("last_seen")
                .and_then(Value::as_str)
                .map(str::to_string);
            let id = next_ticket("LST");
            state.lost_items.push(Ticket {
                id: id.clone(),
                category: item.clone(),
                details: last_seen.clone(),
            });
            ok_envelope(
                format!("Lost-item report opened for '{item}' (ticket {id})."),
                json!({ "ticket_id": id, "item": item, "last_seen": last_seen }),
            )
        }
        HotelIntentKind::RequestBillingSummary => {
            let total = 482.50f64;
            ok_envelope(
                format!("Current folio total: €{total:.2}. Detailed line items attached."),
                json!({
                    "currency": "EUR",
                    "total": total,
                    "lines": [
                        { "description": "Room x 2 nights", "amount": 380.00 },
                        { "description": "Mini bar", "amount": 27.50 },
                        { "description": "Dining", "amount": 75.00 }
                    ]
                }),
            )
        }
        HotelIntentKind::RequestCheckout => {
            let time = args.get("time").and_then(Value::as_str).map(str::to_string);
            ok_envelope(
                "Checkout request received; folio summary will follow.".to_string(),
                json!({ "time": time, "status": "queued" }),
            )
        }
        HotelIntentKind::LanguageHelp => {
            let phrase = args.get("phrase").and_then(Value::as_str).unwrap_or("");
            let target = args
                .get("target_language")
                .and_then(Value::as_str)
                .unwrap_or("italian");
            let translation = mock_translate(phrase, target);
            ok_envelope(
                format!("In {target}: '{translation}'."),
                json!({
                    "phrase": phrase,
                    "target_language": target,
                    "translation": translation,
                }),
            )
        }
    }
}

fn ok_envelope(text: impl Into<String>, structured: Value) -> Value {
    let text = text.into();
    json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false,
        "status": "confirmed",
        "structuredContent": structured,
    })
}

fn error_envelope(reason: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": reason }],
        "isError": true,
    })
}

fn mock_translate(phrase: &str, target: &str) -> String {
    let lower_target = target.to_lowercase();
    match (phrase.to_lowercase().trim(), lower_target.as_str()) {
        ("hello", "italian") => "ciao".to_string(),
        ("thank you", "italian") => "grazie".to_string(),
        ("where is the train station", "italian") => "dov'è la stazione dei treni".to_string(),
        ("hello", "french") => "bonjour".to_string(),
        ("thank you", "french") => "merci".to_string(),
        _ => format!("[{lower_target}] {phrase}"),
    }
}

fn description_for(kind: HotelIntentKind) -> &'static str {
    match kind {
        HotelIntentKind::SetRoomTemperature => {
            "Set the in-room thermostat to a target Celsius value."
        }
        HotelIntentKind::SetLights => "Set lights to a scene and optional brightness.",
        HotelIntentKind::SetCurtains => "Open, close, or position the room curtains.",
        HotelIntentKind::SetTv => "Power, channel, or input control for the room TV.",
        HotelIntentKind::SetAmbientMusic => "Set ambient music mood/volume in the room.",
        HotelIntentKind::SetDoNotDisturb => "Toggle the do-not-disturb status.",
        HotelIntentKind::OrderRoomService => "Place a room-service order.",
        HotelIntentKind::RequestHousekeeping => "Request housekeeping service.",
        HotelIntentKind::RequestExtraTowels => "Request extra towels.",
        HotelIntentKind::RequestExtraPillows => "Request extra pillows.",
        HotelIntentKind::RequestToiletries => "Request additional toiletries.",
        HotelIntentKind::RequestLaundryPickup => "Schedule a laundry pickup.",
        HotelIntentKind::RequestIron => "Send up an iron and/or ironing board.",
        HotelIntentKind::SetWakeUpCall => "Schedule a wake-up call at a specific time.",
        HotelIntentKind::RequestLateCheckout => "Request a late checkout time.",
        HotelIntentKind::BookRestaurant => "Book a table at a hotel restaurant outlet.",
        HotelIntentKind::BookSpa => "Book a spa service at a given time.",
        HotelIntentKind::BookTaxi => "Book a taxi to a destination.",
        HotelIntentKind::ConciergeInfo => "Look up concierge information by topic.",
        HotelIntentKind::RequestLocalRecommendation => {
            "Recommend nearby local options for a topic."
        }
        HotelIntentKind::ReportComplaint => "Log a guest complaint with category and details.",
        HotelIntentKind::ReportLostItem => "Open a lost-item ticket.",
        HotelIntentKind::RequestBillingSummary => "Return the current folio summary.",
        HotelIntentKind::RequestCheckout => "Begin the checkout process.",
        HotelIntentKind::LanguageHelp => "Translate a phrase to the requested target language.",
    }
}

fn schema_for(kind: HotelIntentKind) -> Value {
    match kind {
        HotelIntentKind::SetRoomTemperature => json!({
            "type": "object",
            "properties": { "celsius": { "type": "number" } },
            "required": ["celsius"]
        }),
        HotelIntentKind::SetLights => json!({
            "type": "object",
            "properties": {
                "action": { "type": "string" },
                "scene": { "type": "string" },
                "brightness": { "type": "integer", "minimum": 0, "maximum": 100 }
            }
        }),
        HotelIntentKind::SetCurtains => json!({
            "type": "object",
            "properties": { "action": { "type": "string" }, "position": { "type": "integer" } }
        }),
        HotelIntentKind::SetTv => json!({
            "type": "object",
            "properties": { "action": { "type": "string" }, "channel": { "type": "string" }, "input": { "type": "string" } }
        }),
        HotelIntentKind::SetAmbientMusic => json!({
            "type": "object",
            "properties": { "mood": { "type": "string" }, "volume": { "type": "integer" } }
        }),
        HotelIntentKind::SetDoNotDisturb => json!({
            "type": "object",
            "properties": { "enabled": { "type": "boolean" }, "until": { "type": "string" } },
            "required": ["enabled"]
        }),
        HotelIntentKind::OrderRoomService => json!({
            "type": "object",
            "properties": { "items": { "type": "array", "minItems": 1 } },
            "required": ["items"]
        }),
        HotelIntentKind::RequestHousekeeping => json!({
            "type": "object",
            "properties": { "kind": { "type": "string" }, "when": { "type": "string" } }
        }),
        HotelIntentKind::RequestExtraTowels => json!({
            "type": "object",
            "properties": { "count": { "type": "integer", "minimum": 1 } }
        }),
        HotelIntentKind::RequestExtraPillows => json!({
            "type": "object",
            "properties": { "count": { "type": "integer", "minimum": 1 }, "kind": { "type": "string" } }
        }),
        HotelIntentKind::RequestToiletries => json!({
            "type": "object",
            "properties": { "items": { "type": "array", "items": { "type": "string" } } },
            "required": ["items"]
        }),
        HotelIntentKind::RequestLaundryPickup => json!({
            "type": "object",
            "properties": { "when": { "type": "string" }, "items": { "type": "array" } }
        }),
        HotelIntentKind::RequestIron => json!({
            "type": "object",
            "properties": { "kind": { "type": "string" } }
        }),
        HotelIntentKind::SetWakeUpCall => json!({
            "type": "object",
            "properties": { "time": { "type": "string" } },
            "required": ["time"]
        }),
        HotelIntentKind::RequestLateCheckout => json!({
            "type": "object",
            "properties": { "until": { "type": "string" } }
        }),
        HotelIntentKind::BookRestaurant => json!({
            "type": "object",
            "properties": {
                "outlet": { "type": "string" },
                "time": { "type": "string" },
                "party_size": { "type": "integer", "minimum": 1 }
            },
            "required": ["time"]
        }),
        HotelIntentKind::BookSpa => json!({
            "type": "object",
            "properties": { "service": { "type": "string" }, "time": { "type": "string" } },
            "required": ["service", "time"]
        }),
        HotelIntentKind::BookTaxi => json!({
            "type": "object",
            "properties": {
                "destination": { "type": "string" },
                "time": { "type": "string" },
                "passengers": { "type": "integer", "minimum": 1 }
            },
            "required": ["destination"]
        }),
        HotelIntentKind::ConciergeInfo => json!({
            "type": "object",
            "properties": { "topic": { "type": "string" } },
            "required": ["topic"]
        }),
        HotelIntentKind::RequestLocalRecommendation => json!({
            "type": "object",
            "properties": { "topic": { "type": "string" }, "distance_km": { "type": "number" } },
            "required": ["topic"]
        }),
        HotelIntentKind::ReportComplaint => json!({
            "type": "object",
            "properties": { "category": { "type": "string" }, "details": { "type": "string" } },
            "required": ["category"]
        }),
        HotelIntentKind::ReportLostItem => json!({
            "type": "object",
            "properties": { "item": { "type": "string" }, "last_seen": { "type": "string" } },
            "required": ["item"]
        }),
        HotelIntentKind::RequestBillingSummary => json!({ "type": "object", "properties": {} }),
        HotelIntentKind::RequestCheckout => json!({
            "type": "object",
            "properties": { "time": { "type": "string" } }
        }),
        HotelIntentKind::LanguageHelp => json!({
            "type": "object",
            "properties": {
                "phrase": { "type": "string" },
                "target_language": { "type": "string" },
                "source_language": { "type": "string" }
            },
            "required": ["phrase", "target_language"]
        }),
    }
}
