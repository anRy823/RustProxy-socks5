# 🚀 Getting Started with RustProxy

**Created by [Your Name] - Professional Network Solutions**

Welcome to RustProxy! This guide will get you up and running in just a few minutes.

## 📋 What You Need

- **Windows, Linux, or macOS** computer
- **Internet connection** for testing
- **5 minutes** of your time

## 🎯 Step 1: Download RustProxy

### Option A: Download Pre-built Binary (Recommended)
1. Go to the [Releases page](https://github.com/yourusername/rustproxy/releases)
2. Download the latest version for your operating system
3. Extract the files to a folder (e.g., `C:\RustProxy\`)

### Option B: Build from Source
```bash
git clone https://github.com/yourusername/rustproxy.git
cd rustproxy
cargo build --release
```

## ⚙️ Step 2: Create Configuration

### Windows Users (Easy Way)
1. **Double-click** `start-rustproxy.bat`
2. It will automatically create a `config.toml` file for you
3. **Edit the passwords** in `config.toml` (important for security!)

### Manual Setup
1. **Copy** `config.simple.toml` to `config.toml`
2. **Edit** `config.toml` and change the default passwords:
   ```toml
   [[auth.users]]
   username = "myusername"     # Change this
   password = "mypassword"     # Change this
   enabled = true
   ```

## 🏃 Step 3: Start RustProxy

### Windows
```cmd
rustproxy.exe --config config.toml
```

### Linux/macOS
```bash
./rustproxy --config config.toml
```

### Success! 🎉
You should see:
```
🚀 RustProxy started successfully!
✅ Enterprise SOCKS5 proxy with authentication, access control, and advanced routing
📖 For help and documentation, see USER_MANUAL.md
🛑 Press Ctrl+C or send SIGTERM/SIGINT to shutdown gracefully
```

## 🌐 Step 4: Test Your Proxy

### Quick Test with Curl
```bash
curl --socks5 myusername:mypassword@127.0.0.1:1080 http://httpbin.org/ip
```

This should return your proxy's IP address, not your real IP!

### Configure Your Browser

#### Chrome/Edge:
1. **Settings** → **Advanced** → **System**
2. **"Open your computer's proxy settings"**
3. **Enable** "Use a proxy server"
4. **Address**: `127.0.0.1` **Port**: `1080`
5. **Username**: `myusername` **Password**: `mypassword`

#### Firefox:
1. **Settings** → **Network Settings**
2. **"Manual proxy configuration"**
3. **SOCKS Host**: `127.0.0.1` **Port**: `1080`
4. **SOCKS v5** and **"Proxy DNS when using SOCKS v5"**

## ✅ You're Done!

Your RustProxy is now running and protecting your internet connection!

## 🆘 Need Help?

- **Complete Guide**: See [USER_MANUAL.md](USER_MANUAL.md)
- **Problems?**: Check the [Troubleshooting section](USER_MANUAL.md#troubleshooting)
- **Advanced Features**: See [docs/ADVANCED_ROUTING.md](docs/ADVANCED_ROUTING.md)

## 🔧 Quick Configuration Tips

### Add More Users
```toml
[[auth.users]]
username = "friend1"
password = "friend1pass"
enabled = true

[[auth.users]]
username = "friend2"
password = "friend2pass"
enabled = true
```

### Block Websites
```toml
[[access_control.rules]]
pattern = "*.facebook.com"
action = "block"
reason = "Social media blocked"
```

### Change Port
```toml
[server]
bind_addr = "127.0.0.1:8080"  # Use port 8080 instead of 1080
```

## 🎉 Enjoy RustProxy!

You now have a professional-grade SOCKS5 proxy server running on your computer. 

**Questions?** Contact [Your Name] at [your-email@domain.com]

---

*RustProxy - Created by [Your Name] - Professional Network Solutions*