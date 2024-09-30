use crate::fetcher::{
  search::{search_query_to_object_id, search_query_to_object_id_local, SearchableObjects},
  user_or_community::UserOrCommunity,
};
use activitypub_federation::config::Data;
use actix_web::web::{Json, Query};
use lemmy_api_common::{
  context::LemmyContext,
  site::{ResolveObject, ResolveObjectResponse},
  utils::check_private_instance,
};
use lemmy_db_schema::{source::local_site::LocalSite, utils::DbPool};
use lemmy_db_views::structs::{CommentView, LocalUserView, PostView};
use lemmy_db_views_actor::structs::{CommunityView, PersonView};
use lemmy_utils::error::{LemmyErrorExt2, LemmyErrorType, LemmyResult};

#[tracing::instrument(skip(context))]
pub async fn resolve_object(
  data: Query<ResolveObject>,
  context: Data<LemmyContext>,
  local_user_view: Option<LocalUserView>,
) -> LemmyResult<Json<ResolveObjectResponse>> {
  let local_site = LocalSite::read(&mut context.pool()).await?;
  check_private_instance(&local_user_view, &local_site)?;
  // If we get a valid personId back we can safely assume that the user is authenticated,
  // if there's no personId then the JWT was missing or invalid.
  let is_authenticated = local_user_view.is_some();

  let res = if is_authenticated || cfg!(debug_assertions) {
    // user is fully authenticated; allow remote lookups as well.
    search_query_to_object_id(data.q.clone(), &context).await
  } else {
    // user isn't authenticated only allow a local search.
    search_query_to_object_id_local(&data.q, &context).await
  }
  .with_lemmy_type(LemmyErrorType::NotFound)?;

  convert_response(res, local_user_view, &mut context.pool())
    .await
    .with_lemmy_type(LemmyErrorType::NotFound)
}

async fn convert_response(
  object: SearchableObjects,
  local_user_view: Option<LocalUserView>,
  pool: &mut DbPool<'_>,
) -> LemmyResult<Json<ResolveObjectResponse>> {
  use SearchableObjects::*;
  let mut res = ResolveObjectResponse::default();
  let local_user = local_user_view.map(|l| l.local_user);
  let is_admin = local_user.clone().map(|l| l.admin).unwrap_or_default();

  match object {
    Post(p) => res.post = Some(PostView::read(pool, p.id, local_user.as_ref(), is_admin).await?),
    Comment(c) => res.comment = Some(CommentView::read(pool, c.id, local_user.as_ref()).await?),
    PersonOrCommunity(p) => match *p {
      UserOrCommunity::User(u) => res.person = Some(PersonView::read(pool, u.id).await?),
      UserOrCommunity::Community(c) => {
        res.community = Some(CommunityView::read(pool, c.id, local_user.as_ref(), is_admin).await?)
      }
    },
  };

  Ok(Json(res))
}

#[cfg(test)]
mod tests {
  use crate::api::resolve_object::resolve_object;
  use activitypub_federation::config::Data;
  use actix_web::web::Query;
  use lemmy_api_common::{context::LemmyContext, site::ResolveObject};
  use lemmy_db_schema::{
    newtypes::InstanceId,
    source::{
      community::{Community, CommunityInsertForm},
      instance::Instance,
      local_site::{LocalSite, LocalSiteInsertForm},
      local_user::{LocalUser, LocalUserInsertForm},
      person::{Person, PersonInsertForm},
      post::{Post, PostInsertForm, PostUpdateForm},
      site::{Site, SiteInsertForm},
    },
    traits::Crud,
  };
  use lemmy_db_views::structs::LocalUserView;
  use lemmy_utils::{error::LemmyResult, LemmyErrorType};
  use serial_test::serial;

  async fn create_user(
    instance_id: InstanceId,
    name: String,
    admin: bool,
    context: &Data<LemmyContext>,
  ) -> LemmyResult<LocalUserView> {
    let person_form = PersonInsertForm::test_form(instance_id, &name);
    let person = Person::create(&mut context.pool(), &person_form).await?;

    let user_form = match admin {
      true => LocalUserInsertForm::test_form_admin(person.id),
      false => LocalUserInsertForm::test_form(person.id),
    };
    let local_user = LocalUser::create(&mut context.pool(), &user_form, vec![]).await?;

    Ok(LocalUserView::read(&mut context.pool(), local_user.id).await?)
  }

  #[tokio::test]
  #[serial]
  #[expect(clippy::unwrap_used)]
  async fn test_object_visibility() -> LemmyResult<()> {
    let context = LemmyContext::init_test_context().await;

    let instance = Instance::read_or_create(&mut context.pool(), "example.com".to_string()).await?;

    let site_form = SiteInsertForm::new("test site".to_string(), instance.id);
    let site = Site::create(&mut context.pool(), &site_form).await?;

    let local_site_form = LocalSiteInsertForm {
      site_setup: Some(true),
      private_instance: Some(false),
      ..LocalSiteInsertForm::new(site.id)
    };
    LocalSite::create(&mut context.pool(), &local_site_form).await?;

    let creator = create_user(instance.id, "creator".to_string(), false, &context).await?;
    let regular_user = create_user(instance.id, "user".to_string(), false, &context).await?;
    let admin_user = create_user(instance.id, "admin".to_string(), true, &context).await?;

    //let community_insert_form = ;
    let community = Community::create(
      &mut context.pool(),
      &CommunityInsertForm::new(
        instance.id,
        "test".to_string(),
        "test".to_string(),
        "pubkey".to_string(),
      ),
    )
    .await?;

    let post_insert_form = PostInsertForm::new("Test".to_string(), creator.person.id, community.id);
    let post = Post::create(&mut context.pool(), &post_insert_form).await?;

    let query = format!("q={}", post.ap_id).to_string();
    let query: Query<ResolveObject> = Query::from_query(&query)?;

    // Objects should be resolvable without authentication
    let res = resolve_object(query.clone(), context.reset_request_count(), None).await?;
    assert_eq!(res.post.as_ref().unwrap().post.ap_id, post.ap_id);
    // Objects should be resolvable by regular users
    let res = resolve_object(
      query.clone(),
      context.reset_request_count(),
      Some(regular_user.clone()),
    )
    .await?;
    assert_eq!(res.post.as_ref().unwrap().post.ap_id, post.ap_id);
    // Objects should be resolvable by admins
    let res = resolve_object(
      query.clone(),
      context.reset_request_count(),
      Some(admin_user.clone()),
    )
    .await?;
    assert_eq!(res.post.as_ref().unwrap().post.ap_id, post.ap_id);

    Post::update(
      &mut context.pool(),
      post.id,
      &PostUpdateForm {
        deleted: Some(true),
        ..Default::default()
      },
    )
    .await?;

    // Deleted objects should not be resolvable without authentication
    let res = resolve_object(query.clone(), context.reset_request_count(), None).await;
    assert!(res.is_err_and(|e| e.error_type == LemmyErrorType::NotFound));
    // Deleted objects should not be resolvable by regular users
    let res = resolve_object(
      query.clone(),
      context.reset_request_count(),
      Some(regular_user.clone()),
    )
    .await;
    assert!(res.is_err_and(|e| e.error_type == LemmyErrorType::NotFound));
    // Deleted objects should be resolvable by admins
    let res = resolve_object(
      query.clone(),
      context.reset_request_count(),
      Some(admin_user.clone()),
    )
    .await?;
    assert_eq!(res.post.as_ref().unwrap().post.ap_id, post.ap_id);

    LocalSite::delete(&mut context.pool()).await?;
    Site::delete(&mut context.pool(), site.id).await?;
    Instance::delete(&mut context.pool(), instance.id).await?;

    Ok(())
  }
}
