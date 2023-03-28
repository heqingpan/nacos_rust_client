
## 0.2.1

2022-09-03

1. 禁用 reqwest 的默认依赖。修改 tokio 使用的 features
2. 处理编译时的警告


## 0.2.2 

2023-03-28

1. 修复创建ConfigClient时没有设置auth_addr导致请求认证失败的问题。