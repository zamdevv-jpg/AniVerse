-- AniVerse Supabase setup.
-- The Storage bucket is public so uploaded episodes can stream without a login.
-- Only authenticated users with app_metadata.role = 'admin' can upload or edit.

create table if not exists public.uploaded_episodes (
  id uuid primary key default gen_random_uuid(),
  title text not null,
  slug text not null,
  episode_number text not null,
  synopsis text not null default '',
  poster_path text,
  video_path text not null,
  created_by uuid not null references auth.users(id) on delete restrict,
  created_at timestamptz not null default now()
);

create index if not exists uploaded_episodes_created_at_idx
  on public.uploaded_episodes (created_at desc);

create index if not exists uploaded_episodes_slug_idx
  on public.uploaded_episodes (slug, created_at desc);

-- Public profiles keep display names and Google avatars visible to other users.
create table if not exists public.profiles (
  id uuid primary key references auth.users(id) on delete cascade,
  display_name text not null default 'Pengguna',
  avatar_url text,
  updated_at timestamptz not null default now()
);

alter table public.profiles enable row level security;
grant select on public.profiles to anon, authenticated;

drop policy if exists "Profiles are visible to everyone" on public.profiles;
create policy "Profiles are visible to everyone"
  on public.profiles for select
  using (true);

create or replace function public.sync_aniverse_profile()
returns trigger
language plpgsql
security definer
set search_path = ''
as $$
begin
  insert into public.profiles (id, display_name, avatar_url, updated_at)
  values (
    new.id,
    coalesce(
      nullif(new.raw_user_meta_data ->> 'full_name', ''),
      nullif(new.raw_user_meta_data ->> 'name', ''),
      nullif(split_part(coalesce(new.email, ''), '@', 1), ''),
      'Pengguna'
    ),
    coalesce(
      nullif(new.raw_user_meta_data ->> 'avatar_url', ''),
      nullif(new.raw_user_meta_data ->> 'picture', '')
    ),
    now()
  )
  on conflict (id) do update set
    display_name = excluded.display_name,
    avatar_url = excluded.avatar_url,
    updated_at = excluded.updated_at;
  return new;
end;
$$;

drop trigger if exists on_auth_user_profile_sync on auth.users;
create trigger on_auth_user_profile_sync
  after insert or update of raw_user_meta_data, email on auth.users
  for each row execute function public.sync_aniverse_profile();

insert into public.profiles (id, display_name, avatar_url)
select
  id,
  coalesce(
    nullif(raw_user_meta_data ->> 'full_name', ''),
    nullif(raw_user_meta_data ->> 'name', ''),
    nullif(split_part(coalesce(email, ''), '@', 1), ''),
    'Pengguna'
  ),
  coalesce(
    nullif(raw_user_meta_data ->> 'avatar_url', ''),
    nullif(raw_user_meta_data ->> 'picture', '')
  )
from auth.users
on conflict (id) do update set
  display_name = excluded.display_name,
  avatar_url = excluded.avatar_url,
  updated_at = now();

create table if not exists public.episode_comments (
  id uuid primary key default gen_random_uuid(),
  episode_slug text not null,
  episode_title text not null default '',
  author_id uuid not null constraint episode_comments_author_id_fkey references public.profiles(id) on delete cascade,
  body text not null check (char_length(body) between 1 and 400),
  parent_id uuid references public.episode_comments(id) on delete cascade,
  likes_count integer not null default 0 check (likes_count >= 0),
  created_at timestamptz not null default now()
);

create index if not exists episode_comments_episode_created_idx
  on public.episode_comments (episode_slug, created_at asc);
create index if not exists episode_comments_parent_idx
  on public.episode_comments (parent_id, created_at asc);

alter table public.episode_comments enable row level security;
grant select on public.episode_comments to anon, authenticated;
grant insert on public.episode_comments to authenticated;

drop policy if exists "Anyone can view episode comments" on public.episode_comments;
create policy "Anyone can view episode comments"
  on public.episode_comments for select
  using (true);

drop policy if exists "Signed in users can add episode comments" on public.episode_comments;
create policy "Signed in users can add episode comments"
  on public.episode_comments for insert to authenticated
  with check (
    auth.uid() = author_id
    and (parent_id is null or exists (
      select 1 from public.episode_comments parent
      where parent.id = episode_comments.parent_id
        and parent.episode_slug = episode_comments.episode_slug
    ))
  );

create table if not exists public.episode_comment_likes (
  comment_id uuid not null references public.episode_comments(id) on delete cascade,
  user_id uuid not null references auth.users(id) on delete cascade,
  created_at timestamptz not null default now(),
  primary key (comment_id, user_id)
);

alter table public.episode_comment_likes enable row level security;
grant select, insert, delete on public.episode_comment_likes to authenticated;

drop policy if exists "Users can view their own comment likes" on public.episode_comment_likes;
create policy "Users can view their own comment likes"
  on public.episode_comment_likes for select to authenticated
  using (auth.uid() = user_id);

drop policy if exists "Users can like comments once" on public.episode_comment_likes;
create policy "Users can like comments once"
  on public.episode_comment_likes for insert to authenticated
  with check (
    auth.uid() = user_id
    and exists (select 1 from public.episode_comments c where c.id = episode_comment_likes.comment_id)
  );

drop policy if exists "Users can remove their own comment likes" on public.episode_comment_likes;
create policy "Users can remove their own comment likes"
  on public.episode_comment_likes for delete to authenticated
  using (auth.uid() = user_id);

create or replace function public.update_episode_comment_likes_count()
returns trigger
language plpgsql
security definer
set search_path = ''
as $$
begin
  if tg_op = 'INSERT' then
    update public.episode_comments
      set likes_count = likes_count + 1
      where id = new.comment_id;
    return new;
  end if;
  update public.episode_comments
    set likes_count = greatest(0, likes_count - 1)
    where id = old.comment_id;
  return old;
end;
$$;

drop trigger if exists on_episode_comment_like_count on public.episode_comment_likes;
create trigger on_episode_comment_like_count
  after insert or delete on public.episode_comment_likes
  for each row execute function public.update_episode_comment_likes_count();

alter table public.uploaded_episodes enable row level security;
grant usage on schema public to anon, authenticated;
grant select on public.uploaded_episodes to anon, authenticated;
grant insert, update, delete on public.uploaded_episodes to authenticated;

drop policy if exists "Anyone can view uploaded episodes" on public.uploaded_episodes;
create policy "Anyone can view uploaded episodes"
  on public.uploaded_episodes for select
  using (true);

drop policy if exists "Admins can add uploaded episodes" on public.uploaded_episodes;
create policy "Admins can add uploaded episodes"
  on public.uploaded_episodes for insert to authenticated
  with check ((auth.jwt() -> 'app_metadata' ->> 'role') = 'admin');

drop policy if exists "Admins can edit uploaded episodes" on public.uploaded_episodes;
create policy "Admins can edit uploaded episodes"
  on public.uploaded_episodes for update to authenticated
  using ((auth.jwt() -> 'app_metadata' ->> 'role') = 'admin')
  with check ((auth.jwt() -> 'app_metadata' ->> 'role') = 'admin');

drop policy if exists "Admins can remove uploaded episodes" on public.uploaded_episodes;
create policy "Admins can remove uploaded episodes"
  on public.uploaded_episodes for delete to authenticated
  using ((auth.jwt() -> 'app_metadata' ->> 'role') = 'admin');

insert into storage.buckets (id, name, public, file_size_limit, allowed_mime_types)
values (
  'aniverse-media',
  'aniverse-media',
  true,
  5368709120,
  array['video/mp4', 'video/webm', 'video/quicktime', 'image/jpeg', 'image/png', 'image/webp']
)
on conflict (id) do update set
  public = excluded.public,
  file_size_limit = excluded.file_size_limit,
  allowed_mime_types = excluded.allowed_mime_types;

drop policy if exists "Public can read AniVerse media" on storage.objects;
create policy "Public can read AniVerse media"
  on storage.objects for select
  using (bucket_id = 'aniverse-media');

drop policy if exists "Admins can upload AniVerse media" on storage.objects;
create policy "Admins can upload AniVerse media"
  on storage.objects for insert to authenticated
  with check (
    bucket_id = 'aniverse-media'
    and (auth.jwt() -> 'app_metadata' ->> 'role') = 'admin'
  );

drop policy if exists "Admins can update AniVerse media" on storage.objects;
create policy "Admins can update AniVerse media"
  on storage.objects for update to authenticated
  using (
    bucket_id = 'aniverse-media'
    and (auth.jwt() -> 'app_metadata' ->> 'role') = 'admin'
  )
  with check (
    bucket_id = 'aniverse-media'
    and (auth.jwt() -> 'app_metadata' ->> 'role') = 'admin'
  );

drop policy if exists "Admins can delete AniVerse media" on storage.objects;
create policy "Admins can delete AniVerse media"
  on storage.objects for delete to authenticated
  using (
    bucket_id = 'aniverse-media'
    and (auth.jwt() -> 'app_metadata' ->> 'role') = 'admin'
  );

-- Public profile photos are readable by everyone; each user can manage only
-- files stored under their own auth user ID.
insert into storage.buckets (id, name, public, file_size_limit, allowed_mime_types)
values (
  'aniverse-avatars',
  'aniverse-avatars',
  true,
  2097152,
  array['image/jpeg', 'image/png', 'image/webp']
)
on conflict (id) do update set
  public = excluded.public,
  file_size_limit = excluded.file_size_limit,
  allowed_mime_types = excluded.allowed_mime_types;

drop policy if exists "Public can read AniVerse avatars" on storage.objects;
create policy "Public can read AniVerse avatars"
  on storage.objects for select
  using (bucket_id = 'aniverse-avatars');

drop policy if exists "Users can upload their own AniVerse avatar" on storage.objects;
create policy "Users can upload their own AniVerse avatar"
  on storage.objects for insert to authenticated
  with check (
    bucket_id = 'aniverse-avatars'
    and (storage.foldername(name))[1] = auth.uid()::text
  );

drop policy if exists "Users can update their own AniVerse avatar" on storage.objects;
create policy "Users can update their own AniVerse avatar"
  on storage.objects for update to authenticated
  using (
    bucket_id = 'aniverse-avatars'
    and (storage.foldername(name))[1] = auth.uid()::text
  )
  with check (
    bucket_id = 'aniverse-avatars'
    and (storage.foldername(name))[1] = auth.uid()::text
  );

drop policy if exists "Users can delete their own AniVerse avatar" on storage.objects;
create policy "Users can delete their own AniVerse avatar"
  on storage.objects for delete to authenticated
  using (
    bucket_id = 'aniverse-avatars'
    and (storage.foldername(name))[1] = auth.uid()::text
  );

-- After the user has signed in with Google at least once, grant admin access
-- from the SQL editor by replacing the address with your account email:
-- update auth.users
-- set raw_app_meta_data = coalesce(raw_app_meta_data, '{}'::jsonb) || '{"role":"admin"}'::jsonb
-- where email = 'you@example.com';
