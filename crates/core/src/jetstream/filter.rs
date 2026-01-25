use crate::jetstream::event::{CommitEvent, Facet, FacetFeature, JetstreamEvent};

pub struct JetstreamFilter {
    filter_did: Option<String>,
}

impl JetstreamFilter {
    pub fn new(filter_did: Option<String>) -> Self {
        Self { filter_did }
    }

    pub fn should_process(&self, event: &JetstreamEvent) -> bool {
        match event {
            JetstreamEvent::Commit(commit) => self.should_process_commit(commit),
            _ => false,
        }
    }

    fn should_process_commit(&self, commit: &CommitEvent) -> bool {
        if commit.commit.collection != "app.bsky.feed.post" {
            return false;
        }

        if commit.commit.operation != crate::jetstream::event::Operation::Create {
            return false;
        }

        if let Some(record) = &commit.commit.record
            && let Some(ref did) = self.filter_did
        {
            return self.contains_mention(record, did);
        }

        true
    }

    fn contains_mention(&self, record: &serde_json::Value, did: &str) -> bool {
        if let Some(record_obj) = record.as_object()
            && let Some(facets) = record_obj.get("facets").and_then(|f| f.as_array())
        {
            for facet in facets {
                if let Ok(facet_typed) = serde_json::from_value::<Facet>(facet.clone())
                    && facet_typed
                        .features
                        .iter()
                        .any(|feature| matches!(feature, FacetFeature::Mention { did: d } if d == did))
                {
                    return true;
                }
            }
        }
        false
    }
}
