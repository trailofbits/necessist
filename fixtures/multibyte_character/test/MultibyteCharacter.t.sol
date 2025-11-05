pragma solidity ^0.8.0;

contract MultibyteCharacter {
    // A comment’s multibyte apostrophe.
    function testMultibyteCharacter() public {
        string storage s;
        s = "This candidate should include the trailing semicolon.";
        s = "";
    }
}
