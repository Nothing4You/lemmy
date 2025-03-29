use activitypub_federation::config::Data;
use actix_web::web::Json;
use lemmy_api_common::{
  context::LemmyContext,
  send_activity::{ActivityChannel, SendActivityData},
  site::PurgeCommunity,
  utils::is_admin,
  SuccessResponse,
};
use lemmy_db_schema::{
  source::{
    community::Community,
    moderator::{AdminPurgeCommunity, AdminPurgeCommunityForm},
  },
  traits::Crud,
};
use lemmy_db_views::structs::LocalUserView;
use lemmy_utils::{error::LemmyResult, LemmyErrorType};

#[tracing::instrument(skip(context))]
pub async fn purge_community(
  data: Json<PurgeCommunity>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
) -> LemmyResult<Json<SuccessResponse>> {
  // Only let admin purge an item
  is_admin(&local_user_view)?;

  // Read the community to get its images
  let community = Community::read(&mut context.pool(), data.community_id)
    .await?
    .ok_or(LemmyErrorType::CouldntFindCommunity)?;

  Community::delete(&mut context.pool(), data.community_id).await?;

  // Mod tables
  let form = AdminPurgeCommunityForm {
    admin_person_id: local_user_view.person.id,
    reason: data.reason.clone(),
  };
  AdminPurgeCommunity::create(&mut context.pool(), &form).await?;

  ActivityChannel::submit_activity(
    SendActivityData::RemoveCommunity {
      moderator: local_user_view.person.clone(),
      community,
      reason: data.reason.clone(),
      removed: true,
    },
    &context,
  )
  .await?;

  Ok(Json(SuccessResponse::default()))
}
