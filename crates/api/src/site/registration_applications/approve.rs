use actix_web::web::{Data, Json};
use lemmy_api_common::{
  context::LemmyContext,
  site::{ApproveRegistrationApplication, RegistrationApplicationResponse},
  utils::{is_admin, send_application_approved_email},
};
use lemmy_db_schema::{
  source::{
    local_user::{LocalUser, LocalUserUpdateForm},
    registration_application::{RegistrationApplication, RegistrationApplicationUpdateForm},
  },
  traits::Crud,
  utils::{diesel_string_update, get_conn},
};
use lemmy_db_views::structs::{LocalUserView, RegistrationApplicationView};
use lemmy_utils::{error::LemmyResult, LemmyErrorType};

pub async fn approve_registration_application(
  data: Json<ApproveRegistrationApplication>,
  context: Data<LemmyContext>,
  local_user_view: LocalUserView,
) -> LemmyResult<Json<RegistrationApplicationResponse>> {
  let app_id = data.id;

  // Only let admins do this
  is_admin(&local_user_view)?;

  let pool_context = context.clone();
  let pool = &mut pool_context.pool();
  let conn = &mut get_conn(pool).await?;
  conn
    .build_transaction()
    .run(|conn| {
      Box::pin(async move {
        // Update the registration with reason, admin_id
        let deny_reason = diesel_string_update(data.deny_reason.as_deref());
        let app_form = RegistrationApplicationUpdateForm {
          admin_id: Some(Some(local_user_view.person.id)),
          deny_reason,
        };

        let registration_application =
          RegistrationApplication::update(&mut conn.into(), app_id, &app_form).await?;

        // Update the local_user row
        let local_user_form = LocalUserUpdateForm {
          accepted_application: Some(data.approve),
          ..Default::default()
        };

        let approved_user_id = registration_application.local_user_id;
        LocalUser::update(&mut conn.into(), approved_user_id, &local_user_form).await?;

        if data.approve {
          let approved_local_user_view = LocalUserView::read(&mut conn.into(), approved_user_id)
            .await?
            .ok_or(LemmyErrorType::CouldntFindLocalUser)?;

          if approved_local_user_view.local_user.email.is_some() {
            send_application_approved_email(&approved_local_user_view, context.settings()).await?;
          }
        }

        // Read the view
        let registration_application = RegistrationApplicationView::read(&mut conn.into(), app_id)
          .await?
          .ok_or(LemmyErrorType::CouldntFindRegistrationApplication)?;

        Ok(Json(RegistrationApplicationResponse {
          registration_application,
        }))
      }) as _
    })
    .await
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
  use super::*;
  use lemmy_db_schema::{
    source::{
      // comment::{Comment, CommentInsertForm},
      // comment_reply::{CommentReply, CommentReplyInsertForm, CommentReplyUpdateForm},
      // community::{Community, CommunityInsertForm},
      instance::Instance,
      local_site::LocalSite,
      // local_user::LocalUserInsertForm,
      // person::{Person, PersonInsertForm, PersonUpdateForm},
      // person_block::{PersonBlock, PersonBlockForm},
      // post::{Post, PostInsertForm},
      // site::{Site, SiteInsertForm},
    },
    // utils::DbPool,
  };
  // use lemmy_db_views_actor::{comment_reply_view::CommentReplyQuery,
  // structs::CommentReplyView};
  use serial_test::serial;

  // async fn _create_test_site(pool: &mut DbPool<'_>) -> (Site, Instance) {
  //   let inserted_instance = Instance::read_or_create(pool, "my_domain.tld".to_string())
  //     .await
  //     .unwrap();
  //
  //   let site_form = SiteInsertForm::builder()
  //     .name("test site".to_string())
  //     .instance_id(inserted_instance.id)
  //     .build();
  //   let site = Site::create(pool, &site_form).await.unwrap();
  //
  //   // Create a local site, since this is necessary for local languages
  //   let local_site_form = LocalSiteInsertForm::builder().site_id(site.id).build();
  //   LocalSite::create(pool, &local_site_form).await.unwrap();
  //
  //   (site, inserted_instance)
  // }

  #[tokio::test]
  #[serial]
  async fn test_application_approval() -> LemmyResult<()> {
    let context = LemmyContext::init_test_context().await;
    let pool = &mut context.pool();

    let site = LocalSite::read(pool).await?;
    println!("site: id={:?}, site_id={:?}", site.id, site.site_id);
    println!("instance domain: {:?}", context.settings().hostname);
    let instance = Instance::read_or_create(pool, context.settings().clone().hostname).await?;
    println!("instance: id={:?}", instance.id);

    assert_eq!(true, false, "failing for testing");

    //let (site, instance) = create_test_site(&mut context.pool()).await;

    // let pool = &build_db_pool_for_tests().await;
    // let pool = &mut pool.into();

    // let inserted_instance = Instance::read_or_create(pool, "my_domain.tld".to_string()).await?;
    // LocalSite::

    // let terry_form = PersonInsertForm::test_form(inserted_instance.id, "terrylake");
    // let inserted_terry = Person::create(pool, &terry_form).await?;
    //
    // let recipient_form = PersonInsertForm {
    //   local: Some(true),
    //   ..PersonInsertForm::test_form(inserted_instance.id, "terrylakes recipient")
    // };
    //
    // let inserted_recipient = Person::create(pool, &recipient_form).await?;
    // let recipient_id = inserted_recipient.id;
    //
    // let recipient_local_user =
    //   LocalUser::create(pool, &LocalUserInsertForm::test_form(recipient_id), vec![]).await?;
    //
    // let new_community = CommunityInsertForm::builder()
    //   .name("test community lake".to_string())
    //   .title("nada".to_owned())
    //   .public_key("pubkey".to_string())
    //   .instance_id(inserted_instance.id)
    //   .build();
    //
    // let inserted_community = Community::create(pool, &new_community).await?;
    //
    // let new_post = PostInsertForm::builder()
    //   .name("A test post".into())
    //   .creator_id(inserted_terry.id)
    //   .community_id(inserted_community.id)
    //   .build();
    //
    // let inserted_post = Post::create(pool, &new_post).await?;
    //
    // let comment_form = CommentInsertForm::builder()
    //   .content("A test comment".into())
    //   .creator_id(inserted_terry.id)
    //   .post_id(inserted_post.id)
    //   .build();
    //
    // let inserted_comment = Comment::create(pool, &comment_form, None).await?;
    //
    // let comment_reply_form = CommentReplyInsertForm {
    //   recipient_id: inserted_recipient.id,
    //   comment_id: inserted_comment.id,
    //   read: None,
    // };
    //
    // let inserted_reply = CommentReply::create(pool, &comment_reply_form).await?;
    //
    // let expected_reply = CommentReply {
    //   id: inserted_reply.id,
    //   recipient_id: inserted_reply.recipient_id,
    //   comment_id: inserted_reply.comment_id,
    //   read: false,
    //   published: inserted_reply.published,
    // };
    //
    // let read_reply = CommentReply::read(pool, inserted_reply.id)
    //   .await?
    //   .ok_or(LemmyErrorType::CouldntFindComment)?;
    //
    // let comment_reply_update_form = CommentReplyUpdateForm { read: Some(false) };
    // let updated_reply =
    //   CommentReply::update(pool, inserted_reply.id, &comment_reply_update_form).await?;
    //
    // // Test to make sure counts and blocks work correctly
    // let unread_replies = CommentReplyView::get_unread_replies(pool,
    // &recipient_local_user).await?;
    //
    // let query = CommentReplyQuery {
    //   recipient_id: Some(recipient_id),
    //   my_person_id: Some(recipient_id),
    //   sort: None,
    //   unread_only: false,
    //   show_bot_accounts: true,
    //   page: None,
    //   limit: None,
    // };
    // let replies = query.clone().list(pool).await?;
    // pretty_assertions::assert_eq!(1, unread_replies);
    // pretty_assertions::assert_eq!(1, replies.len());
    //
    // // Block the person, and make sure these counts are now empty
    // let block_form = PersonBlockForm {
    //   person_id: recipient_id,
    //   target_id: inserted_terry.id,
    // };
    // PersonBlock::block(pool, &block_form).await?;
    //
    // let unread_replies_after_block =
    //   CommentReplyView::get_unread_replies(pool, &recipient_local_user).await?;
    // let replies_after_block = query.clone().list(pool).await?;
    // pretty_assertions::assert_eq!(0, unread_replies_after_block);
    // pretty_assertions::assert_eq!(0, replies_after_block.len());
    //
    // // Unblock user so we can reuse the same person
    // PersonBlock::unblock(pool, &block_form).await?;
    //
    // // Turn Terry into a bot account
    // let person_update_form = PersonUpdateForm {
    //   bot_account: Some(true),
    //   ..Default::default()
    // };
    // Person::update(pool, inserted_terry.id, &person_update_form).await?;
    //
    // let recipient_local_user_update_form = LocalUserUpdateForm {
    //   show_bot_accounts: Some(false),
    //   ..Default::default()
    // };
    // LocalUser::update(
    //   pool,
    //   recipient_local_user.id,
    //   &recipient_local_user_update_form,
    // )
    // .await?;
    // let recipient_local_user_view = LocalUserView::read(pool, recipient_local_user.id)
    //   .await?
    //   .ok_or(LemmyErrorType::CouldntFindLocalUser)?;
    //
    // let unread_replies_after_hide_bots =
    //   CommentReplyView::get_unread_replies(pool, &recipient_local_user_view.local_user).await?;
    //
    // let mut query_without_bots = query.clone();
    // query_without_bots.show_bot_accounts = false;
    // let replies_after_hide_bots = query_without_bots.list(pool).await?;
    // pretty_assertions::assert_eq!(0, unread_replies_after_hide_bots);
    // pretty_assertions::assert_eq!(0, replies_after_hide_bots.len());
    //
    // Comment::delete(pool, inserted_comment.id).await?;
    // Post::delete(pool, inserted_post.id).await?;
    // Community::delete(pool, inserted_community.id).await?;
    // Person::delete(pool, inserted_terry.id).await?;
    // Person::delete(pool, inserted_recipient.id).await?;
    // Instance::delete(pool, inserted_instance.id).await?;
    //
    // pretty_assertions::assert_eq!(expected_reply, read_reply);
    // pretty_assertions::assert_eq!(expected_reply, inserted_reply);
    // pretty_assertions::assert_eq!(expected_reply, updated_reply);
    Ok(())
  }
}
