import { Banner } from '@/components/layout'
import { Button } from '@/components/ui/button'
import { Separator } from '@/components/ui/separator'
import { Check, X, Github, Globe, ExternalLink } from 'lucide-react'

interface Contributor {
  name: string
  avatar: string
  links: { type: 'blog' | 'github'; url: string }[]
}

const allowedItems = [
  '个人学习、研究和非盈利使用',
  '修改源代码并用于非盈利用途',
  '在注明出处的前提下进行非商业性质的分发',
]

const forbiddenItems = [
  '将本软件或其源代码用于任何商业 / 盈利用途',
  '销售、倒卖本软件或其衍生作品',
  '将本软件整合到商业产品或服务中',
  '个人利用本软件或其代码进行盈利活动',
]

const acknowledgements = [
  '部分系统镜像及 PE 下载服务由 Cloud-PE 云盘提供',
  '感谢 电脑病毒爱好者 提供 WinPE 及制作宣传视频',
  '以及 Cloud-PE 项目的全体贡献人员',
]

const contributors: Contributor[] = [
  {
    name: 'dddffgg',
    avatar: 'https://pic1.imgdb.cn/item/6906fb8f3203f7be00c2cbc7.png',
    links: [
      { type: 'blog', url: 'https://blog.cloud-pe.cn' },
      { type: 'github', url: 'https://github.com/NORMAL-EX' },
    ],
  },
  {
    name: '电脑病毒爱好者',
    avatar: 'https://pic1.imgdb.cn/item/6961e0d97488ce4061907c41.jpg',
    links: [{ type: 'github', url: 'https://github.com/HelloWin10-19045' }],
  },
  {
    name: 'Hello,World!',
    avatar: 'https://pic1.imgdb.cn/item/6869262058cb8da5c8917549.jpg',
    links: [{ type: 'github', url: 'https://github.com/hwyyds-skidder-team' }],
  },
  {
    name: '普普通通のNeko',
    avatar: 'https://pic1.imgdb.cn/item/6869266b58cb8da5c8917555.jpg',
    links: [],
  },
]

const Section: React.FC<{ title: string; children: React.ReactNode }> = ({ title, children }) => (
  <section className="space-y-4">
    <h2 className="text-lg font-semibold text-foreground">{title}</h2>
    {children}
  </section>
)

const License: React.FC = () => {
  return (
    <>
      <Banner title="许可证说明" subtitle="了解 LetRecovery 的使用条款和版权信息" />

      <section className="py-16 md:py-20">
        <div className="container mx-auto px-4 max-w-3xl space-y-12">
          {/* 基本信息 —— 极简定义列表 */}
          <Section title="关于">
            <dl className="grid grid-cols-[6rem_1fr] gap-y-2 text-sm">
              <dt className="text-muted-foreground">版本</dt>
              <dd className="text-foreground">v2026.2.6</dd>

              <dt className="text-muted-foreground">许可证</dt>
              <dd className="text-foreground">PolyForm Noncommercial 1.0.0</dd>

              <dt className="text-muted-foreground">版权所有</dt>
              <dd className="text-foreground">
                © 2026–present Cloud-PE Dev.
                <br />
                © 2026–present NORMAL-EX.
              </dd>

              <dt className="text-muted-foreground">开源地址</dt>
              <dd>
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-auto px-0 text-primary hover:bg-transparent hover:underline"
                  render={
                    <a
                      href="https://github.com/NORMAL-EX/LetRecovery"
                      target="_blank"
                      rel="noopener noreferrer"
                    />
                  }
                >
                  <Github className="size-4 mr-1.5" />
                  github.com/NORMAL-EX/LetRecovery
                  <ExternalLink className="size-3 ml-1" />
                </Button>
              </dd>
            </dl>
          </Section>

          <Separator />

          {/* 免费声明 —— 单条强调，无多余装饰 */}
          <Section title="免费声明">
            <p className="text-foreground">
              本软件完全免费，禁止任何形式的倒卖行为。
            </p>
            <p className="text-sm text-muted-foreground">
              如果您是通过付费渠道获取本软件，您已被骗，请立即举报并申请退款。
            </p>
          </Section>

          <Separator />

          {/* 使用条款 —— 两列纯文本清单 */}
          <Section title="使用条款">
            <div className="grid sm:grid-cols-2 gap-x-8 gap-y-6">
              <div>
                <h3 className="text-sm font-medium text-foreground mb-3">允许</h3>
                <ul className="space-y-2 text-sm text-muted-foreground">
                  {allowedItems.map((item) => (
                    <li key={item} className="flex gap-2">
                      <Check className="size-4 mt-0.5 shrink-0 text-success" aria-hidden />
                      <span>{item}</span>
                    </li>
                  ))}
                </ul>
              </div>
              <div>
                <h3 className="text-sm font-medium text-foreground mb-3">禁止</h3>
                <ul className="space-y-2 text-sm text-muted-foreground">
                  {forbiddenItems.map((item) => (
                    <li key={item} className="flex gap-2">
                      <X className="size-4 mt-0.5 shrink-0 text-destructive" aria-hidden />
                      <span>{item}</span>
                    </li>
                  ))}
                </ul>
              </div>
            </div>
          </Section>

          <Separator />

          {/* 致谢 —— 纯文本清单 + 简洁贡献者条目 */}
          <Section title="致谢">
            <ul className="space-y-2 text-sm text-muted-foreground">
              {acknowledgements.map((item) => (
                <li key={item} className="flex gap-2">
                  <span className="text-muted-foreground/60 select-none">•</span>
                  <span>{item}</span>
                </li>
              ))}
            </ul>

            <div className="pt-4">
              <h3 className="text-sm font-medium text-foreground mb-4">贡献人员</h3>
              <ul className="space-y-3">
                {contributors.map((c) => (
                  <li key={c.name} className="flex items-center gap-3">
                    <img
                      src={c.avatar}
                      alt={c.name}
                      className="w-9 h-9 rounded-full object-cover"
                      loading="lazy"
                    />
                    <span className="text-sm text-foreground flex-1">{c.name}</span>
                    {c.links.length > 0 && (
                      <div className="flex items-center gap-1">
                        {c.links.map((link) => (
                          <a
                            key={link.url}
                            href={link.url}
                            target="_blank"
                            rel="noopener noreferrer"
                            aria-label={`${c.name} 的 ${link.type}`}
                            className="p-1.5 rounded-md text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
                          >
                            {link.type === 'github' ? (
                              <Github className="size-4" />
                            ) : (
                              <Globe className="size-4" />
                            )}
                          </a>
                        ))}
                      </div>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          </Section>
        </div>
      </section>
    </>
  )
}

export default License
