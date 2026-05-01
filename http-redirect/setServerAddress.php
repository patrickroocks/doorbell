<?php
require_once __DIR__ . "/secrets.inc.php";
if (!isset($_GET['API_KEY']) || $_GET['API_KEY'] !== $API_KEY) {
    http_response_code(401);
    echo "Unauthorized";
    exit;
}

$addr = $_GET['ADDRESS'] ?? '';

if (empty($addr)) {
    http_response_code(400);
    echo "Bad Request: 'ADDRESS' parameter is required";
    exit;
}

$redirectCode = <<<PHP
<?php
header("Location: $addr");
exit;
?>
PHP;

file_put_contents(__DIR__ . "/index.php", $redirectCode);

echo "success";
?>
