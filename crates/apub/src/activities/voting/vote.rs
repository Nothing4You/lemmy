use crate::{
  activities::{
    generate_activity_id,
    verify_person_in_community,
    voting::{undo_vote_comment, undo_vote_post, vote_comment, vote_post},
  },
  insert_received_activity,
  objects::person::ApubPerson,
  protocol::{
    activities::voting::vote::{Vote, VoteType},
    InCommunity,
  },
  PostOrComment,
};
use activitypub_federation::{
  config::Data,
  fetch::object_id::ObjectId,
  traits::{ActivityHandler, Actor},
};
use lemmy_api_common::{context::LemmyContext, utils::check_bot_account};
use lemmy_db_schema::FederationMode;
use lemmy_db_views::structs::SiteView;
use lemmy_utils::error::{LemmyError, LemmyResult};
use url::Url;

impl Vote {
  pub(in crate::activities::voting) fn new(
    object_id: ObjectId<PostOrComment>,
    actor: &ApubPerson,
    kind: VoteType,
    context: &Data<LemmyContext>,
  ) -> LemmyResult<Vote> {
    Ok(Vote {
      actor: actor.id().into(),
      object: object_id,
      kind: kind.clone(),
      id: generate_activity_id(kind, &context.settings().get_protocol_and_hostname())?,
    })
  }
}

#[async_trait::async_trait]
impl ActivityHandler for Vote {
  type DataType = LemmyContext;
  type Error = LemmyError;

  fn id(&self) -> &Url {
    &self.id
  }

  fn actor(&self) -> &Url {
    self.actor.inner()
  }

  async fn verify(&self, context: &Data<LemmyContext>) -> LemmyResult<()> {
    let community = self.community(context).await?;
    verify_person_in_community(&self.actor, &community, context).await?;
    Ok(())
  }

  async fn receive(self, context: &Data<LemmyContext>) -> LemmyResult<()> {
    insert_received_activity(&self.id, context).await?;
    let actor = self.actor.dereference(context).await?;
    let object = self.object.dereference(context).await?;

    check_bot_account(&actor.0)?;

    // Check for enabled federation votes
    let local_site = SiteView::read_local(&mut context.pool())
      .await
      .map(|s| s.local_site)
      .unwrap_or_default();

    let (downvote_setting, upvote_setting) = match object {
      PostOrComment::Post(_) => (local_site.post_downvotes, local_site.post_upvotes),
      PostOrComment::Comment(_) => (local_site.comment_downvotes, local_site.comment_upvotes),
    };

    // Don't allow dislikes for either disabled, or local only votes
    let downvote_fail = self.kind == VoteType::Dislike && downvote_setting != FederationMode::All;
    let upvote_fail = self.kind == VoteType::Like && upvote_setting != FederationMode::All;

    if downvote_fail || upvote_fail {
      // If this is a rejection, undo the vote
      match object {
        PostOrComment::Post(p) => undo_vote_post(actor, &p, context).await,
        PostOrComment::Comment(c) => undo_vote_comment(actor, &c, context).await,
      }
    } else {
      // Otherwise apply the vote normally
      match object {
        PostOrComment::Post(p) => vote_post(&self.kind, actor, &p, context).await,
        PostOrComment::Comment(c) => vote_comment(&self.kind, actor, &c, context).await,
      }
    }
  }
}
