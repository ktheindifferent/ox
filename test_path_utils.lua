-- Test script for path utilities
print("Testing path utilities from Lua...")
print("")

-- Test 1: Path separator
print("1. Path separator test:")
print("   Platform separator: '" .. path_sep .. "'")
print("   Is Windows: " .. tostring(path_utils.is_windows and path_utils:is_windows() or false))
print("")

-- Test 2: build_path function
print("2. build_path test:")
local test_path = build_path("home", "user", "file.txt")
print("   build_path('home', 'user', 'file.txt') = " .. test_path)
print("")

-- Test 3: Path utilities module functions
print("3. Path utilities module tests:")
if path_utils then
    print("   Home directory: " .. tostring(path_utils:home_dir() or "not available"))
    print("   Config directory: " .. tostring(path_utils:config_dir() or "not available"))
    
    -- Test normalize
    local mixed = "home/user\\file.txt"
    print("   Normalize '" .. mixed .. "': " .. tostring(path_utils:normalize(mixed)))
    
    -- Test join
    local joined = path_utils:join({"home", "user", "documents"})
    print("   Join {'home', 'user', 'documents'}: " .. joined)
    
    -- Test expand_home
    local tilde_path = "~/test.txt"
    print("   Expand '" .. tilde_path .. "': " .. path_utils:expand_home(tilde_path))
else
    print("   path_utils module not available")
end
print("")

-- Test 4: Legacy bootstrap functions
print("4. Bootstrap functions test:")
local test_path2 = build_path(home, ".config", "ox")
print("   build_path(home, '.config', 'ox') = " .. test_path2)
print("   file_exists('test.txt'): " .. tostring(file_exists("test.txt")))
print("   plugin_path: " .. plugin_path)
print("")

print("Test completed!")