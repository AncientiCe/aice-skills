use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct StarterChainExample {
    pub phrase: String,
    pub canonical_intent: String,
    pub slots: BTreeMap<String, String>,
}

pub fn starter_chain_examples() -> Vec<StarterChainExample> {
    vec![
        StarterChainExample {
            phrase: "Give me my morning update".to_string(),
            canonical_intent: "daily.update.morning".to_string(),
            slots: BTreeMap::new(),
        },
        StarterChainExample {
            phrase: "Plan my commute update".to_string(),
            canonical_intent: "daily.update.commute".to_string(),
            slots: BTreeMap::new(),
        },
        StarterChainExample {
            phrase: "What's on my schedule and remind me to prep at 5?".to_string(),
            canonical_intent: "assistant.schedule_and_reminder".to_string(),
            slots: BTreeMap::from([
                ("title".to_string(), "prep".to_string()),
                ("when".to_string(), "17:00".to_string()),
            ]),
        },
        StarterChainExample {
            phrase: "Catch me up, then text Alex I'm running late".to_string(),
            canonical_intent: "assistant.catchup_and_message".to_string(),
            slots: BTreeMap::from([
                ("recipient".to_string(), "Alex".to_string()),
                ("message".to_string(), "I am running late".to_string()),
            ]),
        },
        StarterChainExample {
            phrase: "I'd eat some strawberries".to_string(),
            canonical_intent: "shopping_list.add".to_string(),
            slots: BTreeMap::from([("item".to_string(), "strawberries".to_string())]),
        },
        StarterChainExample {
            phrase: "Add strawberries to the shopping list".to_string(),
            canonical_intent: "shopping_list.add".to_string(),
            slots: BTreeMap::from([("item".to_string(), "strawberries".to_string())]),
        },
    ]
}
