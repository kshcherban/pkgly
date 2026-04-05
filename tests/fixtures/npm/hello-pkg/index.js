/**
 * Simple greeting function for integration testing
 * @param {string} name - Name to greet
 * @returns {string} Greeting message
 */
function greet(name) {
    return `Hello, ${name}!`;
}

/**
 * Get package version
 * @returns {string} Version string
 */
function getVersion() {
    return '1.0.0';
}

module.exports = { greet, getVersion };
