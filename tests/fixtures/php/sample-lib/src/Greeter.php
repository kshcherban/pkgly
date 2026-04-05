<?php

namespace PkglyTest\SampleLib;

/**
 * A simple greeter class for integration testing.
 */
class Greeter
{
    /**
     * Returns a greeting message.
     *
     * @param string $name The name to greet
     * @return string A greeting string
     */
    public static function greet(string $name): string
    {
        return "Hello, {$name}!";
    }

    /**
     * Returns the library version.
     *
     * @return string Version string
     */
    public static function getVersion(): string
    {
        return '1.0.0';
    }
}
