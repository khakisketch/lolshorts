begin;

create extension if not exists pgtap with schema extensions;
set local search_path = public, extensions;

select plan(14);

insert into auth.users (id, email)
values
    ('11111111-1111-4111-8111-111111111111', 'rls-user-1@example.test'),
    ('22222222-2222-4222-8222-222222222222', 'rls-user-2@example.test');

set local role authenticated;
set local request.jwt.claim.sub = '11111111-1111-4111-8111-111111111111';
set local request.jwt.claims = '{"sub":"11111111-1111-4111-8111-111111111111","role":"authenticated","email":"rls-user-1@example.test"}';

select results_eq(
    $$select count(*) from public.user_profiles$$,
    array[1::bigint],
    'user 1 sees only their own profile'
);

select results_eq(
    $$select count(*) from public.user_profiles where id = '22222222-2222-4222-8222-222222222222'::uuid$$,
    array[0::bigint],
    'user 1 cannot see user 2 profile'
);

select lives_ok(
    $$update public.user_profiles set display_name = 'User One' where id = '11111111-1111-4111-8111-111111111111'::uuid$$,
    'user 1 can update allowed columns on their own profile'
);

select results_eq(
    $$update public.user_profiles set display_name = 'Compromised' where id = '22222222-2222-4222-8222-222222222222'::uuid returning id$$,
    $$select id from public.user_profiles where false$$,
    'user 1 cannot update user 2 profile'
);

select lives_ok(
    $$insert into public.games (game_id, user_id, game_start_time, champion_name, game_mode) values (1001, '11111111-1111-4111-8111-111111111111'::uuid, now(), 'Ahri', 'CLASSIC')$$,
    'user 1 can insert their own game'
);

select throws_ok(
    $$insert into public.games (game_id, user_id, game_start_time, champion_name, game_mode) values (1002, '22222222-2222-4222-8222-222222222222'::uuid, now(), 'Lux', 'CLASSIC')$$,
    '42501',
    'new row violates row-level security policy for table "games"',
    'user 1 cannot insert a game owned by user 2'
);

select throws_ok(
    $$insert into public.auto_edit_usage (user_id, month, used) values ('11111111-1111-4111-8111-111111111111'::uuid, '2099-01', 99)$$,
    '42501',
    'permission denied for table auto_edit_usage',
    'authenticated clients cannot write the authoritative quota table'
);

select throws_ok(
    $$select * from public.consume_auto_edit_quota('11111111-1111-4111-8111-111111111111'::uuid, '2099-01', 2, 'forged-job')$$,
    '42501',
    'permission denied for function consume_auto_edit_quota',
    'authenticated clients cannot execute the quota RPC directly'
);

reset role;
set local role service_role;

select results_eq(
    $$select allowed, used, "limit" from public.consume_auto_edit_quota('11111111-1111-4111-8111-111111111111'::uuid, '2099-01', 2, 'job-1')$$,
    $$values (true, 1, 2)$$,
    'service role consumes the first quota unit'
);

select results_eq(
    $$select allowed, used, "limit" from public.consume_auto_edit_quota('11111111-1111-4111-8111-111111111111'::uuid, '2099-01', 2, 'job-1')$$,
    $$values (true, 1, 2)$$,
    'replaying the same durable job is idempotent'
);

select results_eq(
    $$select allowed, used, "limit" from public.consume_auto_edit_quota('11111111-1111-4111-8111-111111111111'::uuid, '2099-01', 2, 'job-2')$$,
    $$values (true, 2, 2)$$,
    'a second job consumes the final quota unit'
);

select results_eq(
    $$select allowed, used, "limit" from public.consume_auto_edit_quota('11111111-1111-4111-8111-111111111111'::uuid, '2099-01', 2, 'job-3')$$,
    $$values (false, 2, 2)$$,
    'quota consumption is rejected after the limit'
);

reset role;
set local role authenticated;
set local request.jwt.claim.sub = '11111111-1111-4111-8111-111111111111';
set local request.jwt.claims = '{"sub":"11111111-1111-4111-8111-111111111111","role":"authenticated"}';

select results_eq(
    $$select count(*) from public.auto_edit_usage where month = '2099-01'$$,
    array[1::bigint],
    'user 1 can read their own quota usage'
);

set local request.jwt.claim.sub = '22222222-2222-4222-8222-222222222222';
set local request.jwt.claims = '{"sub":"22222222-2222-4222-8222-222222222222","role":"authenticated"}';

select results_eq(
    $$select count(*) from public.auto_edit_usage where month = '2099-01'$$,
    array[0::bigint],
    'user 2 cannot read user 1 quota usage'
);

select * from finish();
rollback;
