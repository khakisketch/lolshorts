begin;

create extension if not exists pgtap with schema extensions;
set local search_path = public, extensions;

select plan(15);

select is(
    (
        select count(*)
        from pg_class c
        join pg_namespace n on n.oid = c.relnamespace
        where n.nspname = 'public'
          and c.relkind = 'r'
          and c.relname = any(array[
              'user_profiles',
              'license_tiers',
              'user_licenses',
              'subscriptions',
              'payments',
              'games',
              'clips',
              'auto_edit_results',
              'youtube_uploads',
              'quota_usage',
              'billing_events',
              'auto_edit_usage',
              'auto_edit_quota_consumptions'
          ])
    ),
    13::bigint,
    'all application tables are present'
);

select ok(
    (
        select bool_and(c.relrowsecurity)
        from pg_class c
        join pg_namespace n on n.oid = c.relnamespace
        where n.nspname = 'public'
          and c.relkind = 'r'
          and c.relname = any(array[
              'user_profiles',
              'license_tiers',
              'user_licenses',
              'subscriptions',
              'payments',
              'games',
              'clips',
              'auto_edit_results',
              'youtube_uploads',
              'quota_usage',
              'billing_events',
              'auto_edit_usage',
              'auto_edit_quota_consumptions'
          ])
    ),
    'RLS is enabled on every application table'
);

select ok(has_table_privilege('anon', 'public.license_tiers', 'SELECT'), 'anon can read public tier catalog');
select ok(not has_table_privilege('anon', 'public.user_profiles', 'SELECT'), 'anon cannot read user profiles');
select ok(has_table_privilege('authenticated', 'public.user_profiles', 'SELECT'), 'authenticated can read profiles through RLS');
select ok(not has_table_privilege('authenticated', 'public.quota_usage', 'INSERT'), 'authenticated cannot insert legacy quota usage');
select ok(not has_table_privilege('authenticated', 'public.quota_usage', 'UPDATE'), 'authenticated cannot update legacy quota usage');
select ok(not has_table_privilege('authenticated', 'public.auto_edit_usage', 'INSERT'), 'authenticated cannot insert authoritative quota usage');
select ok(not has_table_privilege('authenticated', 'public.auto_edit_usage', 'UPDATE'), 'authenticated cannot update authoritative quota usage');
select ok(not has_table_privilege('authenticated', 'public.auto_edit_quota_consumptions', 'INSERT'), 'authenticated cannot forge quota consumption jobs');
select ok(
    not exists (
        select 1
        from pg_proc p
        cross join lateral aclexplode(coalesce(p.proacl, acldefault('f', p.proowner))) acl
        where p.oid = 'public.consume_auto_edit_quota(uuid,text,integer,text)'::regprocedure
          and acl.grantee = 0
          and acl.privilege_type = 'EXECUTE'
    ),
    'quota RPC is not executable by PUBLIC'
);
select ok(
    not has_function_privilege('authenticated', 'public.consume_auto_edit_quota(uuid,text,integer,text)', 'EXECUTE'),
    'quota RPC is not executable by authenticated clients'
);
select ok(
    has_function_privilege('service_role', 'public.consume_auto_edit_quota(uuid,text,integer,text)', 'EXECUTE'),
    'quota RPC is executable by service_role'
);
select ok(
    not has_function_privilege('authenticated', 'public.handle_new_user()', 'EXECUTE'),
    'auth trigger helper is not executable by authenticated clients'
);
select ok(
    not exists (
        select 1
        from pg_proc p
        cross join lateral aclexplode(coalesce(p.proacl, acldefault('f', p.proowner))) acl
        where p.oid = 'public.handle_new_user()'::regprocedure
          and acl.grantee = 0
          and acl.privilege_type = 'EXECUTE'
    ),
    'auth trigger helper is not executable by PUBLIC'
);

select * from finish();
rollback;
