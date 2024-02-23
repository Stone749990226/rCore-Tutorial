# # 由于每个应用被加载到的位置都不同，也就导致它们的链接脚本 linker.ld 中的 BASE_ADDRESS 都是不同的。实际上，我们不是直接用 cargo build 构建应用的链接脚本，而是写了一个脚本定制工具 build.py ，为每个应用定制了各自的链接脚本：
# import os

# base_address = 0x80400000
# step = 0x20000
# linker = 'src/linker.ld'

# app_id = 0
# apps = os.listdir('src/bin')
# apps.sort()
# for app in apps:
#     app = app[:app.find('.')]
#     lines = []
#     lines_before = []
#     with open(linker, 'r') as f:
#         # 找到 src/linker.ld 中的 BASE_ADDRESS = 0x80400000; 这一行，并将后面的地址替换为和当前应用对应的一个地址；
#         for line in f.readlines():
#             lines_before.append(line)
#             line = line.replace(hex(base_address), hex(base_address+step*app_id))
#             lines.append(line)
#     with open(linker, 'w+') as f:
#         f.writelines(lines)
#     # 找到 src/linker.ld 中的 BASE_ADDRESS = 0x80400000; 这一行，并将后面的地址替换为和当前应用对应的一个地址；
#     os.system('cargo build --bin %s --release' % app)
#     print('[build.py] application %s start with address %s' %(app, hex(base_address+step*app_id)))
#     with open(linker, 'w+') as f:
#         # 将 src/linker.ld 还原
#         f.writelines(lines_before)
#     app_id = app_id + 1
