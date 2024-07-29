use crate::site::registration_applications::{
  approve::approve_registration_application,
  list::list_registration_applications,
  unread_count::get_unread_registration_application_count,
};
use activitypub_federation::config::Data;
use actix_web::web::{Json, Query};
use lemmy_api_common::{
  context::LemmyContext,
  site::{
    ApproveRegistrationApplication,
    EditSite,
    GetUnreadRegistrationApplicationCountResponse,
    ListRegistrationApplicationsResponse,
  },
};
use lemmy_api_crud::site::update::update_site;
use lemmy_db_schema::{
  newtypes::InstanceId,
  source::{
    instance::Instance,
    local_site::{LocalSite, LocalSiteInsertForm},
    local_user::{LocalUser, LocalUserInsertForm, LocalUserUpdateForm},
    person::{Person, PersonInsertForm},
    registration_application::{RegistrationApplication, RegistrationApplicationInsertForm},
    site::{Site, SiteInsertForm},
  },
  traits::Crud,
  utils::DbPool,
  RegistrationMode,
};
use lemmy_db_views::structs::LocalUserView;
use lemmy_utils::error::LemmyResult;
use serial_test::serial;

async fn create_test_site(pool: &mut DbPool<'_>) -> Instance {
  let inserted_instance = Instance::read_or_create(pool, "my_domain.tld".to_string())
    .await
    .expect("Create test instance");

  let site_form = SiteInsertForm::builder()
    .name("test site".to_string())
    .instance_id(inserted_instance.id)
    .build();
  let site = Site::create(pool, &site_form).await.expect("Create site");

  // Create a local site, since this is necessary for determining if email verification is
  // required
  let local_site_form = LocalSiteInsertForm::builder()
    .site_id(site.id)
    .require_email_verification(Some(false))
    .registration_mode(Some(RegistrationMode::RequireApplication))
    .application_question(Some(".".to_string()))
    .build();
  LocalSite::create(pool, &local_site_form)
    .await
    .expect("Create local site");

  inserted_instance
}

async fn signup(
  pool: &mut DbPool<'_>,
  instance_id: InstanceId,
  name: &str,
  email: Option<&str>,
) -> LemmyResult<(LocalUser, RegistrationApplication)> {
  let person_insert_form = PersonInsertForm::test_form(instance_id, name);
  let person = Person::create(pool, &person_insert_form).await?;

  let local_user_insert_form = match email {
    Some(email) => LocalUserInsertForm {
      email: Some(email.to_string()),
      email_verified: Some(false),
      ..LocalUserInsertForm::test_form(person.id)
    },
    None => LocalUserInsertForm::test_form(person.id),
  };

  let local_user = LocalUser::create(pool, &local_user_insert_form, vec![]).await?;

  let application_insert_form = RegistrationApplicationInsertForm {
    local_user_id: local_user.id,
    answer: "x".to_string(),
  };
  let application = RegistrationApplication::create(pool, &application_insert_form).await?;

  Ok((local_user, application))
}

#[allow(clippy::unwrap_used)]
async fn get_application_statuses(
  context: &Data<LemmyContext>,
  admin: LocalUserView,
) -> LemmyResult<(
  Json<GetUnreadRegistrationApplicationCountResponse>,
  Json<ListRegistrationApplicationsResponse>,
  Json<ListRegistrationApplicationsResponse>,
)> {
  let application_count =
    get_unread_registration_application_count(context.reset_request_count(), admin.clone()).await?;

  let unread_applications = list_registration_applications(
    Query::from_query("unread_only=true").unwrap(),
    context.reset_request_count(),
    admin.clone(),
  )
  .await?;

  let all_applications = list_registration_applications(
    Query::from_query("unread_only=false").unwrap(),
    context.reset_request_count(),
    admin,
  )
  .await?;

  Ok((application_count, unread_applications, all_applications))
}

#[allow(clippy::indexing_slicing)]
#[allow(clippy::unwrap_used)]
#[tokio::test]
#[serial]
async fn test_application_approval() -> LemmyResult<()> {
  let context = LemmyContext::init_test_context().await;
  let pool = &mut context.pool();

  let instance = create_test_site(pool).await;

  let admin_person =
    Person::create(pool, &PersonInsertForm::test_form(instance.id, "admin")).await?;
  LocalUser::create(
    pool,
    &LocalUserInsertForm::test_form_admin(admin_person.id),
    vec![],
  )
  .await?;
  let admin_local_user_view = LocalUserView::read_person(pool, admin_person.id)
    .await?
    .unwrap();

  let (local_user_with_email, app_with_email) =
    signup(pool, instance.id, "user_w_email", Some("lemmy@localhost")).await?;
  let (application_count, unread_applications, all_applications) =
    get_application_statuses(&context, admin_local_user_view.clone()).await?;

  // When email verification is required and the email is not verified the application should not
  // be visible to admins
  assert_eq!(0, application_count.registration_applications);
  assert_eq!(0, unread_applications.registration_applications.len());
  assert_eq!(0, all_applications.registration_applications.len());

  LocalUser::update(
    pool,
    local_user_with_email.id,
    &LocalUserUpdateForm {
      email_verified: Some(true),
      ..Default::default()
    },
  )
  .await?;

  let (application_count, unread_applications, all_applications) =
    get_application_statuses(&context, admin_local_user_view.clone()).await?;

  // When email verification is required and the email is verified the application should be
  // visible to admins
  assert_eq!(1, application_count.registration_applications);
  assert_eq!(1, unread_applications.registration_applications.len());
  assert!(
    !unread_applications.registration_applications[0]
      .creator_local_user
      .accepted_application
  );
  assert_eq!(1, all_applications.registration_applications.len());

  approve_registration_application(
    Json(ApproveRegistrationApplication {
      id: app_with_email.id,
      approve: true,
      deny_reason: None,
    }),
    context.reset_request_count(),
    admin_local_user_view.clone(),
  )
  .await?;

  let (application_count, unread_applications, all_applications) =
    get_application_statuses(&context, admin_local_user_view.clone()).await?;

  // When the application is approved it should only be returned for unread queries
  assert_eq!(0, application_count.registration_applications);
  assert_eq!(0, unread_applications.registration_applications.len());
  assert_eq!(1, all_applications.registration_applications.len());
  assert!(
    all_applications.registration_applications[0]
      .creator_local_user
      .accepted_application
  );

  signup(
    pool,
    instance.id,
    "user_w_email_2",
    Some("lemmy2@localhost"),
  )
  .await?;
  let (application_count, unread_applications, all_applications) =
    get_application_statuses(&context, admin_local_user_view.clone()).await?;

  // Email not verified, so application still not visible
  assert_eq!(0, application_count.registration_applications);
  assert_eq!(0, unread_applications.registration_applications.len());
  assert_eq!(1, all_applications.registration_applications.len());

  update_site(
    Json(EditSite {
      require_email_verification: Some(false),
      ..Default::default()
    }),
    context.reset_request_count(),
    admin_local_user_view.clone(),
  )
  .await?;

  let (application_count, unread_applications, all_applications) =
    get_application_statuses(&context, admin_local_user_view.clone()).await?;

  // After disabling email verification the application should now be visible
  assert_eq!(1, application_count.registration_applications);
  assert_eq!(1, unread_applications.registration_applications.len());
  assert_eq!(2, all_applications.registration_applications.len());

  signup(pool, instance.id, "user_wo_email", None).await?;
  let (application_count, unread_applications, all_applications) =
    get_application_statuses(&context, admin_local_user_view.clone()).await?;

  // New user without email should immediately be visible
  assert_eq!(2, application_count.registration_applications);
  assert_eq!(2, unread_applications.registration_applications.len());
  assert_eq!(3, all_applications.registration_applications.len());

  signup(pool, instance.id, "user_w_email_3", None).await?;
  let (application_count, unread_applications, all_applications) =
    get_application_statuses(&context, admin_local_user_view.clone()).await?;

  // New user with email should immediately be visible
  assert_eq!(3, application_count.registration_applications);
  assert_eq!(3, unread_applications.registration_applications.len());
  assert_eq!(4, all_applications.registration_applications.len());

  update_site(
    Json(EditSite {
      registration_mode: Some(RegistrationMode::Open),
      ..Default::default()
    }),
    context.reset_request_count(),
    admin_local_user_view.clone(),
  )
  .await?;
  let (application_count, unread_applications, all_applications) =
    get_application_statuses(&context, admin_local_user_view.clone()).await?;

  // When applications are not required all previous applications should become approved but still
  // visible
  assert_eq!(0, application_count.registration_applications);
  assert_eq!(0, unread_applications.registration_applications.len());
  assert_eq!(4, all_applications.registration_applications.len());

  LocalSite::delete(pool).await?;
  // Instance deletion cascades cleanup of all created persons
  Instance::delete(pool, instance.id).await?;

  Ok(())
}
