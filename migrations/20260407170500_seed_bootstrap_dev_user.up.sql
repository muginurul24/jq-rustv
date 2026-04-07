-- Bootstrap dashboard dev user for local/dev environments.
-- Mirrors the legacy Laravel seeder shape without overwriting an existing user.
INSERT INTO users (
    username,
    name,
    email,
    email_verified_at,
    password,
    role,
    is_active,
    remember_token
)
SELECT
    'justqiuv2',
    'JustQiuV2',
    'justqiuv2@localhost',
    NOW(),
    '$2y$12$UpJ3WaAC3Ut0IFob214Yt.zEJbfk56rP3.8F2iFp.iY227CRsZ1VO',
    'dev',
    true,
    substring(md5('justqiuv2-bootstrap-dev') from 1 for 10)
WHERE NOT EXISTS (
    SELECT 1
    FROM users
    WHERE username = 'justqiuv2'
       OR email = 'justqiuv2@localhost'
);
