pragma solidity ^0.8.0;

contract PureFunction {
    function testPureFunction() public pure {
        string memory s;
        s = "This is a `pure` function.";
        s = "";
    }

    function testViewFunction() public view {
        string memory s;
        s = "This is a `view` function.";
        s = "";
    }
}
