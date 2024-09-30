use activitypub_federation::config::Data;
use lemmy_api_common::context::LemmyContext;
use lemmy_db_schema::{
  newtypes::InstanceId,
  source::{
    local_user::{LocalUser, LocalUserInsertForm},
    person::{Person, PersonInsertForm},
  },
  traits::Crud,
};
use lemmy_db_views::structs::LocalUserView;
use lemmy_utils::error::LemmyResult;

pub async fn create_user(
  instance_id: InstanceId,
  name: String,
  bio: Option<String>,
  admin: bool,
  context: &Data<LemmyContext>,
) -> LemmyResult<LocalUserView> {
  let person_form = PersonInsertForm {
    display_name: Some(name.clone()),
    bio,
    ..PersonInsertForm::test_form(instance_id, &name)
  };
  let person = Person::create(&mut context.pool(), &person_form).await?;

  let user_form = match admin {
    true => LocalUserInsertForm::test_form_admin(person.id),
    false => LocalUserInsertForm::test_form(person.id),
  };
  let local_user = LocalUser::create(&mut context.pool(), &user_form, vec![]).await?;

  Ok(LocalUserView::read(&mut context.pool(), local_user.id).await?)
}
